#!/usr/bin/env python3
"""Interactive authentication launcher for configured AI agents."""
from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import signal
import time
from datetime import datetime, timezone
from glob import glob
from pathlib import Path
from string import Template
from typing import Dict, Iterable, List, Tuple

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_CONFIG_PATH = ROOT / "config" / "agents.json"
CONFIG_ENV_KEY = "AGENTS_CONFIG_PATH"
STATE_ENV_KEY = "AGENTS_AUTH_STATE_DIR"
STATE_FALLBACK_ENV_KEY = "AGENTS_AUTH_STATE_FALLBACK"
DEFAULT_STATE_DIR = ROOT / "state" / "agents"
STATE_FILENAME = "auth_status.json"


def resolve_config_path() -> Path:
    override = os.environ.get(CONFIG_ENV_KEY)
    if override:
        return Path(override).expanduser()
    return DEFAULT_CONFIG_PATH


def writable_dir(candidate: Path) -> Path | None:
    try:
        candidate.mkdir(parents=True, exist_ok=True)
        probe = candidate / ".write-test"
        probe.touch(exist_ok=True)
        probe.unlink(missing_ok=True)
        return candidate
    except PermissionError:
        print(
            f"[agents-auth] нет прав на каталог {candidate} — ищу альтернативу",
            file=sys.stderr,
        )
    except OSError as exc:
        print(
            f"[agents-auth] не удалось подготовить каталог {candidate}: {exc}",
            file=sys.stderr,
        )
    return None


def resolve_state_dir() -> Path:
    override = os.environ.get(STATE_ENV_KEY)
    if override:
        path = writable_dir(Path(override).expanduser())
        if path is not None:
            return path
    preferred = writable_dir(DEFAULT_STATE_DIR)
    if preferred is not None:
        return preferred
    fallback_env = os.environ.get(STATE_FALLBACK_ENV_KEY)
    if fallback_env:
        fallback = writable_dir(Path(fallback_env).expanduser())
        if fallback is not None:
            return fallback
    xdg_state = os.environ.get("XDG_STATE_HOME")
    if xdg_state:
        fallback = writable_dir(Path(xdg_state).expanduser() / "agentcontrol" / "agents")
        if fallback is not None:
            return fallback
    home_fallback = Path.home() / ".local" / "state" / "agentcontrol" / "agents"
    fallback = writable_dir(home_fallback)
    if fallback is not None:
        return fallback
    raise PermissionError("не удалось подобрать каталог для хранения state")


def load_config() -> Dict[str, object]:
    config_path = resolve_config_path()
    if not config_path.exists():
        raise FileNotFoundError(f"config not found: {config_path}")
    with config_path.open("r", encoding="utf-8") as fh:
        return json.load(fh)


def load_state(state_dir: Path) -> Dict[str, object]:
    state_path = state_dir / STATE_FILENAME
    if state_path.exists():
        with state_path.open("r", encoding="utf-8") as fh:
            try:
                return json.load(fh)
            except json.JSONDecodeError:
                print(
                    f"[agents-auth] повреждён state-файл {state_path}, пересоздаю",
                    file=sys.stderr,
                )
    return {"updated_at": None, "agents": {}}


def save_state(state_dir: Path, state: Dict[str, object]) -> None:
    state_path = state_dir / STATE_FILENAME
    state_dir.mkdir(parents=True, exist_ok=True)
    state["updated_at"] = datetime.now(timezone.utc).replace(microsecond=0).isoformat()
    with state_path.open("w", encoding="utf-8") as fh:
        json.dump(state, fh, ensure_ascii=False, indent=2)
        fh.write("\n")


def normalize_command(command: object) -> List[str]:
    if isinstance(command, list):
        return [str(part) for part in command]
    if isinstance(command, str):
        return [command]
    return []


def normalize_env(mapping: object) -> Dict[str, str]:
    if not isinstance(mapping, dict):
        return {}
    env: Dict[str, str] = {}
    for key, value in mapping.items():
        if value is None:
            continue
        env[str(key)] = str(value)
    return env


def expand_path(value: str, env: Dict[str, str]) -> str:
    merged = os.environ.copy()
    merged.update(env)
    templated = Template(value).safe_substitute(merged)
    expanded = os.path.expandvars(templated)
    return os.path.expanduser(expanded)


def iter_credential_sources(entries: object, env: Dict[str, str]) -> Iterable[Path]:
    if entries is None:
        return []
    if isinstance(entries, (str, Path)):
        entries = [entries]
    if not isinstance(entries, list):
        return []
    resolved: List[Path] = []
    for entry in entries:
        if entry is None:
            continue
        path_pattern = expand_path(str(entry), env)
        matches = glob(path_pattern, recursive=True)
        if not matches:
            resolved.append(Path(path_pattern))
            continue
        resolved.extend(Path(match) for match in matches)
    return resolved


def store_credentials(agent: str, cfg: Dict[str, object], env: Dict[str, str], state_dir: Path) -> Tuple[List[str], List[str]]:
    stored: List[str] = []
    warnings: List[str] = []
    targets = iter_credential_sources(cfg.get("credentials_paths"), env)
    agent_state_dir = state_dir / agent
    agent_state_dir.mkdir(parents=True, exist_ok=True)
    for src in targets:
        src_path = src.expanduser().resolve()
        if not src_path.exists():
            warnings.append(f"[agents-auth] {agent}: credential source не найден: {src_path}")
            continue
        try:
            if agent_state_dir in src_path.parents:
                warnings.append(f"[agents-auth] {agent}: пропуск копирования {src_path} (уже внутри state)")
                continue
            if src_path.is_dir():
                dest = agent_state_dir / src_path.name
                if dest.exists():
                    shutil.rmtree(dest)
                shutil.copytree(src_path, dest, dirs_exist_ok=True)
            else:
                dest = agent_state_dir / src_path.name
                dest.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(src_path, dest)
            try:
                stored.append(str(dest.relative_to(ROOT)))
            except ValueError:
                stored.append(str(dest))
        except OSError as exc:
            warnings.append(f"[agents-auth] {agent}: не удалось сохранить {src_path}: {exc}")
    export_cmd = normalize_command(cfg.get("credentials_export_command"))
    if export_cmd:
        export_dest = agent_state_dir / "export.json"
        try:
            result = subprocess.run(
                export_cmd,
                cwd=ROOT,
                env={**os.environ, **env},
                capture_output=True,
                text=True,
                check=False,
            )
            if result.returncode == 0 and result.stdout.strip():
                try:
                    payload = json.loads(result.stdout)
                    export_dest.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
                    try:
                        stored.append(str(export_dest.relative_to(ROOT)))
                    except ValueError:
                        stored.append(str(export_dest))
                except json.JSONDecodeError:
                    warnings.append(f"[agents-auth] {agent}: export командa вернула не-JSON")
            else:
                warnings.append(
                    f"[agents-auth] {agent}: export команда завершилась с кодом {result.returncode}: {result.stderr.strip()}"
                )
        except FileNotFoundError:
            warnings.append(f"[agents-auth] {agent}: export команда не найдена: {' '.join(export_cmd)}")
    return stored, warnings


def credentials_exist(agent: str, state_dir: Path, entry: Dict[str, object]) -> bool:
    paths = entry.get("stored_paths", []) or []
    if not paths:
        return False
    for raw in paths:
        candidate = Path(raw)
        if not candidate.is_absolute():
            candidate = ROOT / candidate
        try:
            if candidate.is_file():
                return True
            if candidate.is_dir():
                # Проверяем наличие хотя бы одного файла внутри каталога
                for child in candidate.rglob("*"):
                    if child.is_file():
                        return True
        except PermissionError as exc:
            print(
                f"[agents-auth] {agent}: нет доступа к {candidate} ({exc})",
                file=sys.stderr,
            )
            continue
    return False


def run_with_auto_exit(command: List[str], cwd: Path, env: Dict[str, str], trigger: str) -> subprocess.CompletedProcess[str]:
    process = subprocess.Popen(
        command,
        cwd=cwd,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        preexec_fn=os.setsid,
    )
    assert process.stdout is not None
    buf: List[str] = []
    login_detected = False
    try:
        while True:
            line = process.stdout.readline()
            if not line:
                break
            print(line, end="")
            buf.append(line)
            if trigger and trigger in line:
                login_detected = True
                pgid = os.getpgid(process.pid)
                for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGKILL):
                    if process.poll() is not None:
                        break
                    try:
                        os.killpg(pgid, sig)
                    except ProcessLookupError:
                        break
                    try:
                        process.wait(timeout=1)
                        break
                    except subprocess.TimeoutExpired:
                        continue
                break
        if login_detected:
            try:
                process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                process.terminate()
        try:
            tail = process.communicate(timeout=10)[0]
            if tail:
                print(tail, end="")
                buf.append(tail)
        except subprocess.TimeoutExpired:
            process.terminate()
            tail = process.communicate()[0]
            if tail:
                print(tail, end="")
                buf.append(tail)
    finally:
        returncode = process.poll()
        if returncode is None:
            process.kill()
            process.wait()
        elif login_detected and returncode in (130, -signal.SIGINT):
            process.returncode = 0
    stdout = "".join(buf)
    return subprocess.CompletedProcess(command, process.returncode or 0, stdout, "")


def run_auth(agent: str, cfg: Dict[str, object], state_dir: Path) -> Dict[str, object]:
    command = normalize_command(cfg.get("auth_command"))
    if not command:
        return {
            "status": "skipped",
            "message": "auth_command not configured",
            "stored_paths": [],
        }
    executable = command[0]
    exec_path = shutil.which(executable) if not Path(executable).is_file() else executable
    if exec_path is None:
        return {
            "status": "skipped",
            "message": f"executable '{executable}' not found",
            "stored_paths": [],
        }
    env_overrides = normalize_env(cfg.get("auth_env"))
    env = os.environ.copy()
    env.update(env_overrides)
    start = datetime.now(timezone.utc).replace(microsecond=0)
    print(f"[agents-auth] Запуск аутентификации для {agent}: {' '.join(command)}")
    auto_exit_trigger = str(cfg.get("auth_auto_exit_trigger", "Successfully logged in")) if cfg.get("auth_auto_exit") else ""
    try:
        if auto_exit_trigger:
            result = run_with_auto_exit(command, ROOT, env, auto_exit_trigger)
        else:
            result = subprocess.run(command, cwd=ROOT, env=env, check=False, capture_output=True, text=True)
            if result.stdout:
                print(result.stdout.strip())
            if result.stderr:
                print(result.stderr.strip(), file=sys.stderr)
    except OSError as exc:
        return {
            "status": "failed",
            "message": f"failed to start auth command: {exc}",
            "stored_paths": [],
            "started_at": start.isoformat(),
        }
    status: Dict[str, object] = {
        "command": command,
        "return_code": result.returncode,
        "started_at": start.isoformat(),
        "finished_at": datetime.now(timezone.utc).replace(microsecond=0).isoformat(),
        "env_overrides": env_overrides,
        "stored_paths": [],
    }
    if result.returncode != 0:
        status["status"] = "failed"
        status["message"] = f"auth exited with {result.returncode}"
        return status
    stored_paths, warnings = store_credentials(agent, cfg, env_overrides, state_dir)
    for warn in warnings:
        print(warn, file=sys.stderr)
    status["status"] = "ok"
    status["stored_paths"] = stored_paths
    return status


def main() -> int:
    try:
        cfg = load_config()
    except FileNotFoundError as exc:
        print(f"[agents-auth] {exc}", file=sys.stderr)
        return 1

    agents_cfg = cfg.get("agents", {})
    if not isinstance(agents_cfg, dict) or not agents_cfg:
        print("[agents-auth] в config/agents.json не найдены агенты", file=sys.stderr)
        return 1

    state_dir = resolve_state_dir()
    state = load_state(state_dir)
    state_agents = state.setdefault("agents", {})

    exit_code = 0
    skipped_count = 0
    total_agents = 0
    for name, data in agents_cfg.items():
        if not isinstance(data, dict):
            continue
        total_agents += 1
        entry_state = state_agents.get(name, {}) if isinstance(state_agents.get(name), dict) else {}
        if entry_state.get("status") == "ok" and credentials_exist(name, state_dir, entry_state):
            print(
                f"[agents-auth] {name}: credentials already present — skipping login",
                file=sys.stderr,
            )
            skipped_count += 1
            continue
        result = run_auth(name, data, state_dir)
        state_agents[name] = result
        if result.get("status") == "failed":
            exit_code = exit_code or 1
        elif result.get("status") == "skipped":
            exit_code = exit_code or 0
    save_state(state_dir, state)
    if skipped_count and skipped_count == total_agents:
        print(
            "[agents-auth] All agents already authenticated. To switch accounts run: agentcall agents logout",
            file=sys.stderr,
        )
    return exit_code


if __name__ == "__main__":
    raise SystemExit(main())

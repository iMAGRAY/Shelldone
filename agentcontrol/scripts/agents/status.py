#!/usr/bin/env python3
"""Display status information for configured AI agents."""
from __future__ import annotations

import argparse
import json
import os
import shutil
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import Dict, List

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_CONFIG_PATH = ROOT / "config" / "agents.json"
DEFAULT_STATE_DIR = ROOT / "state" / "agents"
DEFAULT_LOG_DIR = ROOT / "reports" / "agents"
STATE_FILENAME = "auth_status.json"


@dataclass
class AgentStatus:
    name: str
    cli: str
    cli_exists: bool
    auth_status: str
    auth_message: str
    credentials_ok: bool
    credentials_paths: List[str]
    last_log: str
    last_command: str
    last_timestamp: str


def resolve_config_path() -> Path:
    override = os.environ.get("AGENTS_CONFIG_PATH")
    if override:
        return Path(override).expanduser()
    return DEFAULT_CONFIG_PATH


def resolve_state_dir() -> Path:
    override = os.environ.get("AGENTS_AUTH_STATE_DIR")
    if override:
        return Path(override).expanduser()
    fallback = os.environ.get("AGENTS_AUTH_STATE_FALLBACK")
    if fallback:
        return Path(fallback).expanduser()
    return DEFAULT_STATE_DIR


def resolve_log_dir(config: Dict[str, object]) -> Path:
    value = config.get("log_dir")
    if isinstance(value, str):
        path = Path(value)
        if not path.is_absolute():
            return (ROOT / path).resolve()
        return path
    return DEFAULT_LOG_DIR


def load_config(path: Path) -> Dict[str, object]:
    if not path.exists():
        raise FileNotFoundError(f"config not found: {path}")
    with path.open("r", encoding="utf-8") as fh:
        return json.load(fh)


def load_state(state_dir: Path) -> Dict[str, object]:
    state_path = state_dir / STATE_FILENAME
    if not state_path.exists():
        return {"agents": {}}
    with state_path.open("r", encoding="utf-8") as fh:
        return json.load(fh)


def resolve_cli(agent_cfg: Dict[str, object]) -> tuple[str, bool]:
    command = agent_cfg.get("command")
    if isinstance(command, list) and command:
        candidate = command[0]
    elif isinstance(command, str):
        candidate = command
    else:
        return "<unconfigured>", False
    candidate_path = Path(candidate)
    if candidate_path.is_file():
        return str(candidate_path), True
    resolved = shutil.which(candidate)
    if resolved:
        return resolved, True
    return candidate, False


def path_exists(path_str: str) -> bool:
    path = Path(path_str)
    if not path.is_absolute():
        path = (ROOT / path).resolve()
    try:
        if path.is_file():
            return True
        if path.is_dir():
            for child in path.rglob("*"):
                if child.is_file():
                    return True
    except PermissionError:
        return False
    return False


def last_log_entry(log_dir: Path, agent: str | None) -> tuple[str, str, str]:
    if not log_dir.exists():
        return ("", "", "")
    files = sorted(log_dir.glob("*.log"))
    if agent:
        files = [f for f in files if f.name.count("-") >= 2 and f.name.split("-")[1] == agent]
    if not files:
        return ("", "", "")
    last = files[-1]
    parts = last.stem.split("-", 2)
    timestamp = parts[0] if parts else ""
    agent_name = parts[1] if len(parts) > 1 else ""
    command = parts[2] if len(parts) > 2 else ""
    return (str(last), command, timestamp)


def collect_status() -> List[AgentStatus]:
    config_path = resolve_config_path()
    config = load_config(config_path)
    agents_cfg = config.get("agents", {})
    if not isinstance(agents_cfg, dict):
        return []
    state_dir = resolve_state_dir()
    state = load_state(state_dir)
    state_agents = state.get("agents", {}) if isinstance(state.get("agents"), dict) else {}
    log_dir = resolve_log_dir(config)

    results: List[AgentStatus] = []
    for name, cfg in agents_cfg.items():
        if not isinstance(cfg, dict):
            continue
        cli_path, cli_exists = resolve_cli(cfg)
        state_entry = state_agents.get(name, {}) if isinstance(state_agents.get(name), dict) else {}
        auth_status = state_entry.get("status", "unknown")
        auth_message = state_entry.get("message", "")
        stored_paths = [str(p) for p in state_entry.get("stored_paths", []) or []]
        credentials_ok = any(path_exists(p) for p in stored_paths)
        log_file, last_command, last_ts = last_log_entry(log_dir, name)
        results.append(
            AgentStatus(
                name=name,
                cli=cli_path,
                cli_exists=cli_exists,
                auth_status=str(auth_status),
                auth_message=str(auth_message),
                credentials_ok=credentials_ok,
                credentials_paths=stored_paths,
                last_log=log_file,
                last_command=last_command,
                last_timestamp=last_ts,
            )
        )
    return results


def short(path: str, missing: bool = False) -> str:
    if not path:
        return ""
    name = Path(path).name
    return name if not missing else f"{name}!"


def render_table(rows: List[AgentStatus]) -> str:
    if not rows:
        return "Нет сконфигурированных агентов."
    headers = ["Agent", "Auth", "Creds", "CLI", "Last"]
    data = []
    for row in rows:
        auth = row.auth_status
        if row.auth_message:
            auth += f" ({row.auth_message})"
        creds = "ok" if row.credentials_ok else "missing"
        cli = short(row.cli, not row.cli_exists)
        last = ""
        if row.last_timestamp or row.last_command:
            last = f"{row.last_timestamp} {row.last_command}".strip()
        data.append([row.name, auth, creds, cli, last])
    widths = [max(len(str(col)) for col in column) for column in zip(headers, *data)]
    lines = []
    lines.append(" | ".join(str(h).ljust(widths[i]) for i, h in enumerate(headers)))
    lines.append("-+-".join("-" * w for w in widths))
    for row in data:
        lines.append(" | ".join(str(cell).ljust(widths[i]) for i, cell in enumerate(row)))
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser(description="Show agent status")
    parser.add_argument("--json", action="store_true", help="output JSON")
    args = parser.parse_args()
    rows = collect_status()
    json_mode = args.json or os.environ.get("STATUS_JSON") == "1"
    if json_mode:
        payload = [
            {
                "name": row.name,
                "cli": row.cli,
                "cli_exists": row.cli_exists,
                "auth_status": row.auth_status,
                "auth_message": row.auth_message,
                "credentials_ok": row.credentials_ok,
                "credentials_paths": row.credentials_paths,
                "last_log": row.last_log,
                "last_command": row.last_command,
                "last_timestamp": row.last_timestamp,
            }
            for row in rows
        ]
        print(json.dumps(payload, ensure_ascii=False, indent=2))
        return 0
    print(render_table(rows))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

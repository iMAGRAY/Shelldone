#!/usr/bin/env python3
"""Remove stored credentials for configured AI agents."""
from __future__ import annotations

import json
import os
import shutil
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Dict, Iterable

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_CONFIG_PATH = ROOT / "config" / "agents.json"
STATE_ENV_KEY = "AGENTS_AUTH_STATE_DIR"
STATE_FALLBACK_ENV_KEY = "AGENTS_AUTH_STATE_FALLBACK"
DEFAULT_STATE_DIR = ROOT / "state" / "agents"
STATE_FILENAME = "auth_status.json"


def resolve_config_path() -> Path:
    override = os.environ.get("AGENTS_CONFIG_PATH")
    if override:
        return Path(override).expanduser()
    return DEFAULT_CONFIG_PATH


def resolve_state_dir() -> Path:
    override = os.environ.get(STATE_ENV_KEY)
    if override:
        return Path(override).expanduser()
    if DEFAULT_STATE_DIR.exists():
        return DEFAULT_STATE_DIR
    fallback = os.environ.get(STATE_FALLBACK_ENV_KEY)
    if fallback:
        return Path(fallback).expanduser()
    xdg = os.environ.get("XDG_STATE_HOME")
    if xdg:
        return Path(xdg).expanduser() / "agentcontrol" / "agents"
    return Path.home() / ".local" / "state" / "agentcontrol" / "agents"


def load_config() -> Dict[str, object]:
    with resolve_config_path().open("r", encoding="utf-8") as fh:
        return json.load(fh)


def load_state(state_dir: Path) -> Dict[str, object]:
    state_path = state_dir / STATE_FILENAME
    if not state_path.exists():
        return {"updated_at": None, "agents": {}}
    with state_path.open("r", encoding="utf-8") as fh:
        return json.load(fh)


def save_state(state_dir: Path, data: Dict[str, object]) -> None:
    state_path = state_dir / STATE_FILENAME
    state_dir.mkdir(parents=True, exist_ok=True)
    data["updated_at"] = datetime.now(timezone.utc).replace(microsecond=0).isoformat()
    with state_path.open("w", encoding="utf-8") as fh:
        json.dump(data, fh, ensure_ascii=False, indent=2)
        fh.write("\n")


def iter_paths(entries: Iterable[str]) -> Iterable[Path]:
    for raw in entries:
        candidate = Path(raw)
        if not candidate.is_absolute():
            candidate = ROOT / candidate
        yield candidate


def remove_path(path: Path) -> bool:
    try:
        if path.is_symlink() or path.is_file():
            path.unlink(missing_ok=True)
            return True
        if path.is_dir():
            shutil.rmtree(path)
            return True
    except FileNotFoundError:
        return False
    except PermissionError:
        print(f"[agents-auth-logout] недостаточно прав для {path}", file=sys.stderr)
    return False


def clear_agent(agent: str, state_dir: Path, entry: Dict[str, object]) -> Dict[str, object]:
    removed = 0
    for path in iter_paths(entry.get("stored_paths", []) or []):
        if remove_path(path):
            removed += 1
    agent_dir = state_dir / agent
    if remove_path(agent_dir):
        removed += 1
    return {
        "status": "logged_out",
        "message": f"credentials cleared ({removed} paths removed)",
        "stored_paths": [],
        "cleared_at": datetime.now(timezone.utc).replace(microsecond=0).isoformat(),
    }


def main() -> int:
    cfg = load_config()
    state_dir = resolve_state_dir()
    state = load_state(state_dir)
    current_agents = state.setdefault("agents", {})
    agents_cfg = cfg.get("agents", {})
    if not isinstance(agents_cfg, dict) or not agents_cfg:
        print("[agents-auth-logout] no agents defined", file=sys.stderr)
        return 1

    for name in agents_cfg.keys():
        entry_state = current_agents.get(name)
        if not isinstance(entry_state, dict):
            entry_state = {}
        current_agents[name] = clear_agent(name, state_dir, entry_state)
        print(f"[agents-auth-logout] {name}: credentials cleared")
    save_state(state_dir, state)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

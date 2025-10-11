#!/usr/bin/env python3
"""Inspect agent execution logs."""
from __future__ import annotations

import argparse
import json
import os
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, List

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_CONFIG_PATH = ROOT / "config" / "agents.json"
DEFAULT_LOG_DIR = ROOT / "reports" / "agents"


@dataclass
class LogEntry:
    path: Path
    agent: str
    command: str
    timestamp: str


def resolve_config_path() -> Path:
    override = os.environ.get("AGENTS_CONFIG_PATH")
    if override:
        return Path(override).expanduser()
    return DEFAULT_CONFIG_PATH


def load_config(path: Path) -> dict:
    with path.open("r", encoding="utf-8") as fh:
        return json.load(fh)


def resolve_log_dir(config: dict) -> Path:
    value = config.get("log_dir")
    if isinstance(value, str):
        path = Path(value)
        if not path.is_absolute():
            return (ROOT / path).resolve()
        return path
    return DEFAULT_LOG_DIR


def discover_logs(log_dir: Path, agent: str | None = None) -> List[LogEntry]:
    if not log_dir.exists():
        return []
    entries: List[LogEntry] = []
    for path in sorted(log_dir.glob("*.log")):
        parts = path.stem.split("-", 2)
        if len(parts) < 3:
            continue
        timestamp, agent_name, command = parts
        if agent and agent_name != agent:
            continue
        entries.append(LogEntry(path=path, agent=agent_name, command=command, timestamp=timestamp))
    return entries


def format_list(entries: Iterable[LogEntry]) -> str:
    rows = list(entries)
    if not rows:
        return "Логи не найдены."

    widths = [8, 10, 10, 40]
    header = "{:<8}  {:<10}  {:<10}  {}".format("#", "Timestamp", "Agent", "Command")
    lines = [header, "-" * len(header)]
    for idx, entry in enumerate(rows, start=1):
        lines.append(
            "{:<8}  {:<10}  {:<10}  {}".format(
                idx,
                entry.timestamp,
                entry.agent,
                entry.command,
            )
        )
    return "\n".join(lines)


def show_entry(entry: LogEntry) -> str:
    try:
        content = entry.path.read_text(encoding="utf-8")
    except FileNotFoundError:
        return f"Log not found: {entry.path}"
    header = f"--- {entry.timestamp} {entry.agent} {entry.command} ({entry.path}) ---"
    return f"{header}\n{content.strip()}\n{'-' * len(header)}"


def main() -> int:
    parser = argparse.ArgumentParser(description="Inspect agent logs")
    parser.add_argument("--agent", help="filter logs by agent")
    parser.add_argument("--last", type=int, default=-1, help="show last N logs")
    parser.add_argument("--list", action="store_true", help="list available logs")
    args = parser.parse_args()

    agent = args.agent or os.environ.get('AGENT')
    last = args.last if args.last >= 0 else int(os.environ.get('LAST', '1'))
    list_only = args.list or os.environ.get('LIST') == '1'

    config = load_config(resolve_config_path())
    log_dir = resolve_log_dir(config)
    logs = discover_logs(log_dir, agent)

    if list_only or last <= 0:
        print(format_list(logs))
        return 0

    subset = logs[-last:]
    if not subset:
        print("Логи не найдены.")
        return 0

    for entry in subset:
        print(show_entry(entry))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

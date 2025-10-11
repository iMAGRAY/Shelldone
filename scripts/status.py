#!/usr/bin/env python3
"""Aggregate snapshot reader for reports/status.json (project-local)."""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any, Dict

PROJECT_ROOT = Path(__file__).resolve().parents[1]
STATUS_PATH = PROJECT_ROOT / "reports" / "status.json"


def load_status() -> Dict[str, Any]:
    if not STATUS_PATH.exists():
        return {}
    try:
        return json.loads(STATUS_PATH.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:  # pragma: no cover - corrupted file
        raise SystemExit(f"Invalid status.json: {exc}")


def summarise(status: Dict[str, Any]) -> Dict[str, Any]:
    roadmap = status.get("roadmap", {})
    program = roadmap.get("program", {})
    warnings = roadmap.get("warnings", [])
    tasks = status.get("tasks", {})
    return {
        "program": {
            "name": program.get("name"),
            "progress_pct": program.get("progress_pct"),
            "manual_progress_pct": program.get("manual_progress_pct"),
            "health": program.get("health"),
            "phase_progress": program.get("phase_progress", {}),
            "milestones": program.get("milestones", []),
        },
        "warnings": warnings,
        "tasks": {
            "counts": tasks.get("counts", {}),
            "updated_at": tasks.get("updated_at"),
            "board_version": tasks.get("board_version"),
        },
        "generated_at": status.get("generated_at"),
    }


def format_text(summary: Dict[str, Any]) -> str:
    program = summary["program"]
    lines = [
        f"Program: {program.get('name', 'n/a')} {program.get('progress_pct', 0)}%"
        f" (manual {program.get('manual_progress_pct', 0)}%, health {program.get('health', 'n/a')})",
        "Phase progress:",
    ]
    phase_progress = program.get("phase_progress", {})
    for phase, value in phase_progress.items():
        lines.append(f"  - {phase}: {value}%")
    milestones = program.get("milestones", [])
    if milestones:
        lines.append("Milestones:")
        for milestone in milestones:
            lines.append(
                f"  - {milestone.get('title', 'n/a')} (due {milestone.get('due', 'n/a')}): {milestone.get('status', 'planned')}"
            )
    warnings = summary.get("warnings", [])
    if warnings:
        lines.append("Warnings:")
        for warning in warnings:
            lines.append(f"  - {warning}")
    tasks = summary.get("tasks", {})
    counts = tasks.get("counts", {})
    if counts:
        lines.append("Tasks:")
        for status, count in sorted(counts.items()):
            lines.append(f"  - {status}: {count}")
    board_version = tasks.get("board_version")
    if board_version:
        lines.append(f"Board version: {board_version} (updated_at={tasks.get('updated_at', 'n/a')})")
    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser(description="Shelldone status snapshot reader")
    parser.add_argument("--json", action="store_true", help="Emit raw status JSON")
    parser.add_argument(
        "--summary-json",
        action="store_true",
        help="Emit structured summary JSON instead of text",
    )
    args = parser.parse_args()

    status = load_status()
    if args.json:
        json.dump(status, sys.stdout, indent=2, sort_keys=True)
        sys.stdout.write("\n")
        return

    summary = summarise(status)
    if args.summary_json:
        json.dump(summary, sys.stdout, indent=2, sort_keys=True)
        sys.stdout.write("\n")
        return

    sys.stdout.write(format_text(summary) + "\n")


if __name__ == "__main__":
    main()

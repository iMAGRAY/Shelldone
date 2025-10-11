#!/usr/bin/env python3
"""Quick guardrail for progress warnings, task board staleness, and Heart freshness."""
from __future__ import annotations

import argparse
import json
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, List


def _load_roadmap_helpers(project_root: Path):
    project_path = str(project_root)
    if project_path not in sys.path:
        sys.path.insert(0, project_path)
    # Import roadmap_status helpers without external SDKs
    import importlib

    mod = importlib.import_module("scripts.roadmap_status")
    return mod


def _parse_iso(dt: str | None) -> datetime | None:
    if not dt:
        return None
    try:
        return datetime.fromisoformat(dt.replace("Z", "+00:00"))
    except ValueError:
        return None


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--project",
        type=Path,
        default=Path(__file__).resolve().parents[1],
        help="Project root (default: repository root inferred from script location)",
    )
    parser.add_argument("--json", action="store_true", help="Emit JSON instead of text")
    parser.add_argument(
        "--max-heart-age-hours",
        type=float,
        default=6.0,
        help="Maximum acceptable age of Memory Heart index before warning",
    )
    parser.add_argument(
        "--max-board-age-hours",
        type=float,
        default=24.0,
        help="Maximum acceptable age of task board update before warning",
    )
    parser.add_argument(
        "--max-termbridge-age-hours",
        type=float,
        default=12.0,
        help="Maximum acceptable age of TermBridge capability snapshot before warning",
    )
    return parser


def _check_health(
    project_root: Path,
    max_heart_age_hours: float,
    max_board_age_hours: float,
    max_termbridge_age_hours: float,
) -> Dict[str, Any]:
    roadmap = _load_roadmap_helpers(project_root)
    program, epics, tasks = roadmap.load_todo()
    computed_progress = roadmap.compute_program_progress(epics, tasks)
    manual_progress = float(program.progress)

    warnings: List[str] = []
    now = datetime.now(timezone.utc)

    # Governance capsule no longer required; rely on local reports/state only.

    # Optional board snapshot (manual process); tolerate absence
    board_file = project_root / "reports" / "status.json"
    board_updated_at = None
    if board_file.exists():
        try:
            snap = json.loads(board_file.read_text(encoding="utf-8"))
            board = snap.get("tasks", {})
            board_updated_at = _parse_iso(board.get("updated_at"))
        except Exception:
            pass
    board_age_hours: float | None = None
    if board_updated_at is None:
        warnings.append("Task board updated_at missing; update board per docs/governance/status-updates.md")
    else:
        board_age_hours = (now - board_updated_at).total_seconds() / 3600.0
        if board_age_hours > max_board_age_hours:
            warnings.append(
                f"Task board stale ({board_age_hours:.1f}h > {max_board_age_hours}h); review reports/status.json and docs/governance/status-updates.md"
            )

    heart_manifest = project_root / "state" / "heart" / "manifest.json"
    heart_generated_at = None
    heart_age_hours: float | None = None
    if heart_manifest.exists():
        manifest_data = json.loads(heart_manifest.read_text(encoding="utf-8"))
        heart_generated_at = _parse_iso(manifest_data.get("generated_at"))
        if heart_generated_at is not None:
            heart_age_hours = (now - heart_generated_at).total_seconds() / 3600.0
            if heart_age_hours > max_heart_age_hours:
                warnings.append(
                    f"Memory Heart index is {heart_age_hours:.1f}h old (> {max_heart_age_hours}h); refresh Heart index"
                )
        else:
            warnings.append("Memory Heart manifest missing generated_at timestamp")
    else:
        warnings.append("Memory Heart index missing; refresh Heart index")

    approvals_path = project_root / "state" / "approvals" / "pending.json"
    pending_approvals = 0
    if approvals_path.exists():
        try:
            approvals_data = json.loads(approvals_path.read_text(encoding="utf-8"))
            pending_approvals = sum(
                1
                for entry in approvals_data
                if entry.get("status") in (None, "pending")
            )
            if not approvals_data:
                warnings.append(
                    "Local approvals snapshot empty; ensure status refreshed and HTTP /approvals/pending reachable"
                )
        except Exception as exc:
            warnings.append(f"Failed to read approvals file: {exc}")
    
    termbridge_capabilities = project_root / "state" / "termbridge" / "capabilities.json"
    termbridge_age_hours: float | None = None
    if termbridge_capabilities.exists():
        mtime = datetime.fromtimestamp(
            termbridge_capabilities.stat().st_mtime, timezone.utc
        )
        termbridge_age_hours = (now - mtime).total_seconds() / 3600.0
        if termbridge_age_hours > max_termbridge_age_hours:
            warnings.append(
                f"TermBridge capability snapshot is {termbridge_age_hours:.1f}h old (> {max_termbridge_age_hours}h); run `make status` or refresh discovery"
            )
    else:
        warnings.append(
            "TermBridge capability snapshot missing; run `make status` to seed state/termbridge/capabilities.json"
        )

    return {
        "program": {
            "computed_progress_pct": computed_progress,
            "manual_progress_pct": manual_progress,
            "health": "ok" if abs(computed_progress - manual_progress) <= 0.5 else "drift",
        },
        "phase_progress": {},
        "board": {
            "counts": {},
            "updated_at": board_updated_at.isoformat() if board_updated_at else None,
            "age_hours": board_age_hours,
        },
        "heart": {
            "manifest_path": heart_manifest.as_posix(),
            "generated_at": heart_generated_at.isoformat() if heart_generated_at else None,
            "age_hours": heart_age_hours,
        },
        "approvals": {
            "pending_count": pending_approvals,
        },
        "termbridge": {
            "path": termbridge_capabilities.as_posix(),
            "age_hours": termbridge_age_hours,
        },
        "warnings": warnings,
    }


def main(argv: list[str] | None = None) -> int:
    parser = _build_parser()
    args = parser.parse_args(argv)

    project_root: Path = args.project.resolve()
    report = _check_health(
        project_root,
        args.max_heart_age_hours,
        args.max_board_age_hours,
        args.max_termbridge_age_hours,
    )
    warnings = report["warnings"]

    if args.json:
        print(json.dumps(report, ensure_ascii=False, indent=2))
    else:
        program = report["program"]
        print(
            f"Program: {program['computed_progress_pct']}% computed (manual {program['manual_progress_pct']}%), health={program['health']}"
        )
        board = report["board"]
        board_age = board["age_hours"]
        board_age_str = "n/a" if board_age is None else f"{board_age:.1f}h"
        print(f"Task board: updated_at={board['updated_at']} age={board_age_str}")
        heart = report["heart"]
        heart_age = heart["age_hours"]
        heart_age_str = "n/a" if heart_age is None else f"{heart_age:.1f}h"
        print(f"Heart index: generated_at={heart['generated_at']} age={heart_age_str}")
        termbridge = report["termbridge"]
        termbridge_age = termbridge["age_hours"]
        termbridge_age_str = "n/a" if termbridge_age is None else f"{termbridge_age:.1f}h"
        print(f"TermBridge snapshot: path={termbridge['path']} age={termbridge_age_str}")
        if warnings:
            print("Warnings:")
            for item in warnings:
                print(f"- {item}")
        else:
            print("Warnings: none")

    return 1 if warnings else 0


if __name__ == "__main__":
    raise SystemExit(main())

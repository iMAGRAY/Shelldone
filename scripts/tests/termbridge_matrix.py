#!/usr/bin/env python3
"""TermBridge compatibility matrix test harness.

Currently a placeholder that validates discovery JSON layout until full
terminal automation is wired. The script exits non-zero on schema drift.
"""

from __future__ import annotations

import json
import pathlib
import sys


def load_capability_map(path: pathlib.Path) -> dict:
    data = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(data, dict):
        raise ValueError("capability map root must be an object")
    return data


def validate(snapshot: dict) -> None:
    required_keys = {"terminals", "generated_at"}
    missing = required_keys - snapshot.keys()
    if missing:
        raise ValueError(f"missing keys in capability map: {sorted(missing)}")
    if not isinstance(snapshot["terminals"], list):
        raise ValueError("terminals must be a list")
    for entry in snapshot["terminals"]:
        for key in ("terminal", "capabilities", "requires_opt_in"):
            if key not in entry:
                raise ValueError(f"terminal entry missing '{key}'")


def main() -> int:
    repo_root = pathlib.Path(__file__).resolve().parents[2]
    artifacts_dir = repo_root / "artifacts" / "termbridge"
    snapshot_path = artifacts_dir / "capability-map.json"
    if not snapshot_path.exists():
        print(f"warning: {snapshot_path} not found; skipping schema check", file=sys.stderr)
        return 0
    snapshot = load_capability_map(snapshot_path)
    validate(snapshot)
    return 0


if __name__ == "__main__":
    sys.exit(main())

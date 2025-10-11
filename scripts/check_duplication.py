#!/usr/bin/env python3
"""
Detect duplicate task identifiers and roadmap log directories.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent


def extract_ids(text: str) -> list[str]:
    pattern = re.compile(r"\bid\s*:\s*([A-Za-z0-9_.\-]+)")
    return pattern.findall(text)


def check_unique(items: list[str], source: Path) -> None:
    seen: dict[str, int] = {}
    duplicates: list[str] = []
    for item in items:
        seen[item] = seen.get(item, 0) + 1
        if seen[item] == 2:
            duplicates.append(item)
    if duplicates:
        dup_list = ", ".join(sorted(duplicates))
        raise SystemExit(
            f"[dup] duplicate identifiers detected in {source}: {dup_list}"
        )


def main() -> None:
    tasks_path = ROOT / "docs" / "tasks.yaml"
    if tasks_path.exists():
        ids = extract_ids(tasks_path.read_text(encoding="utf-8"))
        check_unique(ids, tasks_path)

    log_refs: set[str] = set()
    log_dirs_pattern = re.compile(r"log_dir:\s*([^\n]+)")
    for match in log_dirs_pattern.findall(tasks_path.read_text(encoding="utf-8")):
        value = match.strip()
        log_refs.add(value)

    missing: list[str] = []
    for ref in sorted(log_refs):
        target = ROOT / ref
        if not target.exists():
            missing.append(ref)
    if missing:
        raise SystemExit(
            "[dup] referenced RFT log directories are missing: "
            + ", ".join(missing)
        )

    print("[dup] ids and directories look good")


if __name__ == "__main__":
    try:
        main()
    except SystemExit as exc:
        raise
    except Exception as exc:
        print(f"[dup] unexpected error: {exc}", file=sys.stderr)
        raise SystemExit(1)

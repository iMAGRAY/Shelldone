#!/usr/bin/env python3
"""Report Shelldone roadmap readiness based on todo.machine.md."""
from __future__ import annotations

import argparse
import json
from collections import Counter
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, Iterable, List, Sequence

import yaml

ROOT = Path(__file__).resolve().parent.parent
TODO_PATH = ROOT / "todo.machine.md"


class RoadmapError(RuntimeError):
    """Raised when roadmap validation fails."""


@dataclass
class Program:
    raw: Dict[str, object]

    @property
    def progress(self) -> float:
        return float(self.raw.get("progress_pct", 0))


@dataclass
class Epic:
    raw: Dict[str, object]

    @property
    def id(self) -> str:
        return str(self.raw["id"])

    @property
    def size(self) -> int:
        return int(self.raw["size_points"])

    @property
    def declared_progress(self) -> float:
        return float(self.raw.get("progress_pct", 0))


@dataclass
class BigTask:
    raw: Dict[str, object]

    @property
    def id(self) -> str:
        return str(self.raw["id"])

    @property
    def epic(self) -> str:
        return str(self.raw["parent_epic"])

    @property
    def size(self) -> int:
        return int(self.raw["size_points"])

    @property
    def progress(self) -> float:
        return float(self.raw.get("progress_pct", 0))


# ----------------------------------------------------------------------------
# Markdown helpers (stay aligned with scripts/verify.py)
# ----------------------------------------------------------------------------

def _section_lines(text: str, header: str) -> List[str]:
    lines = text.splitlines()
    capture = False
    collected: List[str] = []
    needle = f"## {header}"
    for line in lines:
        if line.strip() == needle:
            capture = True
            continue
        if capture and line.startswith("## "):
            break
        if capture:
            collected.append(line)
    if not collected:
        raise RoadmapError(f"Section '{header}' is missing in todo.machine.md")
    return collected


def _extract_yaml_blocks(section_lines: Sequence[str]) -> List[str]:
    blocks: List[str] = []
    collecting = False
    current: List[str] = []
    for line in section_lines:
        stripped = line.strip()
        if not collecting and stripped.startswith("```yaml"):
            collecting = True
            current = []
            continue
        if collecting and stripped.startswith("```"):
            blocks.append("\n".join(current).strip())
            collecting = False
            continue
        if collecting:
            current.append(line)
    if collecting:
        raise RoadmapError("Unclosed YAML block detected")
    return [block for block in blocks if block]


def _load_yaml_blocks(blocks: Iterable[str]) -> List[Dict[str, object]]:
    items: List[Dict[str, object]] = []
    for raw in blocks:
        data = yaml.safe_load(raw)  # type: ignore[arg-type]
        if not isinstance(data, dict):
            raise RoadmapError("Expected YAML block to contain a mapping")
        items.append(data)
    return items


# ----------------------------------------------------------------------------
# todo.machine.md parsing
# ----------------------------------------------------------------------------

def load_todo() -> tuple[Program, List[Epic], List[BigTask]]:
    if not TODO_PATH.exists():
        raise RoadmapError("todo.machine.md is missing")
    text = TODO_PATH.read_text(encoding="utf-8")

    program_raw = _load_yaml_blocks(_extract_yaml_blocks(_section_lines(text, "Program")))
    if len(program_raw) != 1:
        raise RoadmapError("Program section must contain exactly one YAML block")
    program = Program(program_raw[0])

    epics_raw = _load_yaml_blocks(_extract_yaml_blocks(_section_lines(text, "Epics")))
    epics = [Epic(item) for item in epics_raw]

    tasks_raw = _load_yaml_blocks(_extract_yaml_blocks(_section_lines(text, "Big Tasks")))
    tasks = [BigTask(item) for item in tasks_raw]

    return program, epics, tasks


# ----------------------------------------------------------------------------
# Calculations
# ----------------------------------------------------------------------------

def compute_epic_progress(epic: Epic, tasks: Sequence[BigTask]) -> float:
    related = [task for task in tasks if task.epic == epic.id]
    if not related:
        return 0.0
    total = sum(task.size for task in related)
    if total == 0:
        return 0.0
    weighted = sum(task.size * task.progress for task in related)
    return round(weighted / total, 2)


def compute_program_progress(epics: Sequence[Epic], tasks: Sequence[BigTask]) -> float:
    if not epics:
        return 0.0
    total = sum(epic.size for epic in epics)
    if total == 0:
        return 0.0
    weighted = 0.0
    for epic in epics:
        weighted += epic.size * compute_epic_progress(epic, tasks)
    return round(weighted / total, 2)


# ----------------------------------------------------------------------------
# Presentation
# ----------------------------------------------------------------------------

def format_table(epics: Sequence[Epic], tasks: Sequence[BigTask]) -> List[str]:
    rows: List[str] = []
    header = ["Epic", "Declared %", "Calculated %", "Δ", "Size", "Status"]
    rows.append(header)
    for epic in epics:
        calc = compute_epic_progress(epic, tasks)
        declared = round(epic.declared_progress, 2)
        delta = round(calc - declared, 2)
        status = str(epic.raw.get("status", "?"))
        rows.append(
            [
                epic.id,
                f"{declared:.2f}",
                f"{calc:.2f}",
                f"{delta:+.2f}",
                str(epic.size),
                status,
            ]
        )
    widths = [max(len(row[i]) for row in rows) for i in range(len(header))]
    formatted: List[str] = []
    for idx, row in enumerate(rows):
        line = "  ".join(col.ljust(widths[i]) for i, col in enumerate(row))
        formatted.append(line)
        if idx == 0:
            formatted.append("  ".join("-" * widths[i] for i in range(len(header))))
    return formatted


def build_json(
    program: Program,
    epics: Sequence[Epic],
    tasks: Sequence[BigTask],
    program_delta: float,
    epic_deltas: Dict[str, float],
) -> Dict[str, object]:
    epic_entries = []
    for epic in epics:
        epic_entries.append(
            {
                "id": epic.id,
                "size_points": epic.size,
                "declared_progress_pct": round(epic.declared_progress, 2),
                "calculated_progress_pct": compute_epic_progress(epic, tasks),
                "delta_progress_pct": epic_deltas[epic.id],
                "status": epic.raw.get("status"),
                "priority": epic.raw.get("priority"),
            }
        )
    status_counter = Counter(str(task.raw.get("status", "?")) for task in tasks)
    return {
        "program": {
            "declared_progress_pct": round(program.progress, 2),
            "calculated_progress_pct": compute_program_progress(epics, tasks),
            "delta_progress_pct": program_delta,
            "epic_count": len(epics),
            "task_count": len(tasks),
        },
        "epics": epic_entries,
        "tasks": {
            "total": len(tasks),
            "status_breakdown": dict(status_counter),
        },
    }


def _should_fail(program_delta: float, epic_deltas: Dict[str, float], threshold: float = 0.5) -> bool:
    if abs(program_delta) > threshold:
        return True
    return any(abs(delta) > threshold for delta in epic_deltas.values())


def _task_status_lines(tasks: Sequence[BigTask]) -> List[str]:
    status_counter = Counter(str(task.raw.get("status", "?")) for task in tasks)
    total = len(tasks)
    lines = ["Task status breakdown:"]
    for status, count in sorted(status_counter.items(), key=lambda kv: kv[0]):
        percent = (count / total * 100) if total else 0.0
        lines.append(f"  - {status}: {count}/{total} ({percent:.2f}%)")
    done = status_counter.get("done", 0)
    done_pct = (done / total * 100) if total else 0.0
    lines.append(f"Completed big tasks: {done}/{total} ({done_pct:.2f}%)")
    return lines


def main() -> int:
    parser = argparse.ArgumentParser(description="Roadmap readiness reporter")
    parser.add_argument("--json", action="store_true", help="print machine-readable JSON output")
    parser.add_argument(
        "--strict",
        action="store_true",
        help="fail when progress drift exceeds 0.5 percentage points",
    )
    args = parser.parse_args()

    try:
        program, epics, tasks = load_todo()
    except RoadmapError as err:
        print(f"Error: {err}")
        return 1

    calculated_program = compute_program_progress(epics, tasks)
    declared_program = round(program.progress, 2)
    program_delta = round(calculated_program - declared_program, 2)
    epic_deltas = {
        epic.id: round(compute_epic_progress(epic, tasks) - round(epic.declared_progress, 2), 2)
        for epic in epics
    }

    if args.json:
        payload = build_json(program, epics, tasks, program_delta, epic_deltas)
        print(json.dumps(payload, indent=2, ensure_ascii=True))
        return 0 if not (args.strict and _should_fail(program_delta, epic_deltas)) else 2

    print("Shelldone roadmap status")
    print(
        f"Program readiness: {calculated_program:.2f}% (declared {declared_program:.2f}%, Δ {program_delta:+.2f} pp)"
    )
    print(f"Epics: {len(epics)}, big tasks: {len(tasks)}")
    print()
    for line in format_table(epics, tasks):
        print(line)
    print()
    for line in _task_status_lines(tasks):
        print(line)

    if args.strict and _should_fail(program_delta, epic_deltas):
        print("\n⚠️  Drift exceeds 0.5 percentage points", flush=True)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

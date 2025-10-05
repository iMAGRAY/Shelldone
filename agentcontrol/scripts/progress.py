#!/usr/bin/env python3
"""Recompute and persist program progress projections."""
from __future__ import annotations

import argparse
import sys
from copy import deepcopy
from pathlib import Path
from typing import Dict, List, Tuple

import yaml

PROJECT_ROOT = Path(__file__).resolve().parents[2]
if str(PROJECT_ROOT) not in sys.path:
    sys.path.append(str(PROJECT_ROOT))


from agentcontrol.app.progress import ProgressProjectionService
from agentcontrol.domain.progress import ProgramProgressAggregate


def _snapshot() -> Tuple[ProgressProjectionService, ProgramProgressAggregate]:
    service = ProgressProjectionService.default(str(PROJECT_ROOT))
    aggregate = service.compute()
    return service, aggregate


def _extract_section(text: str, section: str) -> Tuple[Dict[str, object] | list[dict], Tuple[int, int]]:
    marker = f"## {section}\n```yaml\n"
    start = text.find(marker)
    if start == -1:
        raise SystemExit(f"Секция '{section}' не найдена в todo.machine.md")
    block_start = start + len(marker)
    end_marker = "\n```"
    block_end = text.find(end_marker, block_start)
    if block_end == -1:
        raise SystemExit(f"Секция '{section}' оформлена некорректно")
    body = text[block_start:block_end]
    data = yaml.safe_load(body) or {}
    return data, (block_start, block_end + len(end_marker))


def _replace_section(text: str, section: str, data: object) -> str:
    marker = f"## {section}\n```yaml\n"
    start = text.find(marker)
    if start == -1:
        raise SystemExit(f"Секция '{section}' не найдена при перезаписи")
    block_start = start + len(marker)
    end_marker = "\n```"
    block_end = text.find(end_marker, block_start)
    if block_end == -1:
        raise SystemExit(f"Секция '{section}' оформлена некорректно")
    dumped = yaml.dump(data, sort_keys=False, allow_unicode=True, width=1000).strip()
    return text[:block_start] + dumped + end_marker + text[block_end + len(end_marker):]


def _update_todo(text: str, manifest: dict) -> str:
    program_block, program_span = _extract_section(text, "Program")
    epics_block, epics_span = _extract_section(text, "Epics")
    big_tasks_block, big_span = _extract_section(text, "Big Tasks")

    program_copy = deepcopy(program_block)
    program_meta = manifest.get("program", {})
    progress = program_meta.get("progress", {})
    meta = program_meta.get("meta", {})
    program_copy["progress_pct"] = progress.get("progress_pct", program_copy.get("progress_pct", 0))
    program_copy["health"] = progress.get("health", program_copy.get("health", "yellow"))
    program_copy["phase_progress"] = progress.get("phase_progress", program_copy.get("phase_progress", {}))
    program_copy["milestones"] = program_meta.get("milestones", program_copy.get("milestones", []))
    program_copy["updated_at"] = meta.get("updated_at", program_copy.get("updated_at"))

    epics_index = {epic.get("id"): epic for epic in manifest.get("epics", [])}
    updated_epics = []
    if not isinstance(epics_block, list):
        raise SystemExit("Секция Epics должна быть YAML-списком")
    for epic in epics_block:
        epic_id = epic.get("id")
        manifest_epic = epics_index.get(epic_id)
        if not manifest_epic:
            updated_epics.append(epic)
            continue
        epic_copy = deepcopy(epic)
        metrics = manifest_epic.get("metrics", {})
        epic_copy["progress_pct"] = metrics.get("progress_pct", manifest_epic.get("progress_pct", epic_copy.get("progress_pct", 0)))
        epic_copy["status"] = manifest_epic.get("status", epic_copy.get("status", "planned"))
        epic_copy["health"] = manifest_epic.get("health", epic_copy.get("health", "yellow"))
        updated_epics.append(epic_copy)

    big_index = {bt.get("id"): bt for bt in manifest.get("big_tasks", [])}
    updated_big = []
    if not isinstance(big_tasks_block, list):
        raise SystemExit("Секция Big Tasks должна быть YAML-списком")
    for bt in big_tasks_block:
        bt_id = bt.get("id")
        manifest_bt = big_index.get(bt_id)
        if not manifest_bt:
            updated_big.append(bt)
            continue
        bt_copy = deepcopy(bt)
        metrics = manifest_bt.get("metrics", {})
        bt_copy["progress_pct"] = metrics.get("progress_pct", manifest_bt.get("progress_pct", bt_copy.get("progress_pct", 0)))
        bt_copy["status"] = manifest_bt.get("status", bt_copy.get("status", "planned"))
        bt_copy["health"] = manifest_bt.get("health", bt_copy.get("health", "yellow"))
        updated_big.append(bt_copy)

    new_text = text
    new_text = _replace_section(new_text, "Program", program_copy)
    new_text = _replace_section(new_text, "Epics", updated_epics)
    new_text = _replace_section(new_text, "Big Tasks", updated_big)
    return new_text


def _render_table(title: str, headers: list[str], rows: list[list[str]]) -> str:
    widths = [len(header) for header in headers]
    for row in rows:
        for idx, cell in enumerate(row):
            widths[idx] = max(widths[idx], len(cell))

    def border(char: str) -> str:
        return "+" + "+".join(char * (width + 2) for width in widths) + "+"

    def render_row(cells: list[str]) -> str:
        return "|" + "|".join(f" {cell.ljust(widths[idx])} " for idx, cell in enumerate(cells)) + "|"

    table = [title, border("-"), render_row(headers), border("=")]
    for row in rows:
        table.append(render_row(row))
    table.append(border("-"))
    return "\n".join(table)


def _render_summary(manifest: dict) -> str:
    program_meta = manifest.get("program", {})
    progress = program_meta.get("progress", {})
    name = program_meta.get("meta", {}).get("name", "Program")
    health = progress.get("health", "yellow")
    pct = f"{progress.get('progress_pct', 0)}%"
    updated = program_meta.get("meta", {}).get("updated_at", "n/a")
    program_table = _render_table("Программа", ["Название", "Состояние", "Прогресс", "Обновлено"], [[name, health, pct, updated]])

    epic_rows = []
    for epic in manifest.get("epics", []):
        metrics = epic.get("metrics", {})
        epic_rows.append([
            str(epic.get("id")),
            epic.get("title", ""),
            epic.get("status", "planned"),
            f"{metrics.get('progress_pct', epic.get('progress_pct', 0))}%",
            str(epic.get("size_points", 0)),
        ])
    epic_table = _render_table("Эпики", ["ID", "Название", "Статус", "Прогресс", "Размер"], epic_rows) if epic_rows else ""

    big_rows = []
    for bt in manifest.get("big_tasks", []):
        metrics = bt.get("metrics", {})
        big_rows.append([
            str(bt.get("id")),
            bt.get("title", ""),
            bt.get("status", "planned"),
            f"{metrics.get('progress_pct', bt.get('progress_pct', 0))}%",
            bt.get("parent_epic", ""),
            str(bt.get("size_points", 0)),
        ])
    big_table = _render_table(
        "Big Tasks",
        ["ID", "Название", "Статус", "Прогресс", "Эпик", "Размер"],
        big_rows,
    ) if big_rows else ""

    sections = [program_table]
    if epic_table:
        sections.append("")
        sections.append(epic_table)
    if big_table:
        sections.append("")
        sections.append(big_table)
    return "\n".join(sections).strip()


def run(dry_run: bool = False) -> None:
    service, aggregate = _snapshot()

    manifest_projection = service.build_manifest_projection()
    manifest_changed = manifest_projection != service.current_manifest()

    todo_text = service.load_todo_text()
    updated_todo = _update_todo(todo_text, manifest_projection)
    todo_changed = updated_todo != todo_text

    status_payload = service.build_status_payload()

    if dry_run:
        print(f"Program computed progress: {aggregate.computed_progress.value}% (manual {aggregate.manual_progress.value}%)")
        for warning in aggregate.warnings:
            print(f"WARN: {warning}")
        return

    if todo_changed:
        service.save_todo_text(updated_todo)
        print("Обновлён todo.machine.md")
    else:
        print("todo.machine.md уже актуален")

    if manifest_changed:
        service.save_manifest(manifest_projection)
        print("Обновлён architecture/manifest.yaml")
    else:
        print("manifest.yaml без изменений")

    service.save_status(status_payload)
    print("Обновлён agentcontrol/reports/status.json")

    print(_render_summary(manifest_projection))
    if aggregate.warnings:
        print("\nПредупреждения:")
        for warning in aggregate.warnings:
            print(f"- {warning}")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Пересчитать прогресс Shelldone")
    parser.add_argument("--dry-run", action="store_true", help="Показать вычисленные значения без записи")
    args = parser.parse_args(argv)
    run(dry_run=args.dry_run)
    return 0


def load_manifest() -> dict:
    """Return manifest projection with computed metrics (read-only)."""
    service, _ = _snapshot()
    return service.build_manifest_projection()


def calculate_progress(manifest: dict | None = None) -> Tuple[dict, List[dict], List[dict], Dict[str, int]]:
    """Expose structured progress metrics for agent tooling."""
    service, aggregate = _snapshot()
    if manifest is None:
        manifest = service.current_manifest()

    program = {
        "name": aggregate.name,
        "computed_pct": aggregate.computed_progress.value,
        "manual_pct": aggregate.manual_progress.value,
        "health": aggregate.health,
        "generated_at": aggregate.generated_at.isoformat(),
    }

    epics: List[dict] = [
        {
            "id": epic.epic_id,
            "title": epic.title,
            "status": epic.status,
            "progress_pct": epic.computed.value,
            "manual_pct": epic.manual.value,
            "size_points": float(epic.size_points),
        }
        for epic in aggregate.epics
    ]

    big_tasks: List[dict] = [
        {
            "id": big.big_task_id,
            "title": big.title,
            "parent_epic": big.parent_epic,
            "status": big.status,
            "progress_pct": big.computed.value,
            "manual_pct": big.manual.value,
            "size_points": float(big.size_points),
        }
        for big in aggregate.big_tasks
    ]

    phase_progress = {title: value.value for title, value in aggregate.phase_progress.items()}
    return program, epics, big_tasks, phase_progress


def render_progress_tables(
    program: dict,
    epics: List[dict],
    big_tasks: List[dict],
    manifest: dict | None = None,
) -> str:
    projection = manifest or load_manifest()
    return _render_summary(projection)


def collect_progress_state() -> dict:
    """Return a concise snapshot for agent orchestration."""
    service, aggregate = _snapshot()
    board = service.task_board()
    return {
        "program": {
            "name": aggregate.name,
            "progress_pct": aggregate.computed_progress.value,
            "manual_progress_pct": aggregate.manual_progress.value,
            "health": aggregate.health,
            "generated_at": aggregate.generated_at.isoformat(),
        },
        "phase_progress": {title: value.value for title, value in aggregate.phase_progress.items()},
        "epics": [
            {
                "id": epic.epic_id,
                "title": epic.title,
                "status": epic.status,
                "progress_pct": epic.computed.value,
            }
            for epic in aggregate.epics
        ],
        "big_tasks": [
            {
                "id": big.big_task_id,
                "title": big.title,
                "parent_epic": big.parent_epic,
                "status": big.status,
                "progress_pct": big.computed.value,
            }
            for big in aggregate.big_tasks
        ],
        "board": {
            "counts": service.board_counts(),
            "updated_at": board.get("updated_at"),
            "version": board.get("version"),
        },
        "warnings": list(aggregate.warnings),
    }


if __name__ == "__main__":
    raise SystemExit(main())

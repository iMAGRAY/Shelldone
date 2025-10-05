#!/usr/bin/env python3
"""Architecture manifest tooling for GPT-5 Codex SDK."""
from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Dict, List, Tuple

import yaml

CURRENT_DIR = Path(__file__).resolve().parent
PROJECT_ROOT = CURRENT_DIR.parents[1]
if str(PROJECT_ROOT) not in sys.path:
    sys.path.append(str(PROJECT_ROOT))

from scripts.lib.progress_utils import (
    PHASE_ORDER,
    compute_phase_progress,
    status_from_progress,
    status_score,
    utc_now_iso,
    weighted_numeric_average,
    weighted_status_average,
)

ROOT = Path(__file__).resolve().parents[2]
MANIFEST_PATH = ROOT / "architecture" / "manifest.yaml"
STATE_DIR = ROOT / ".sdk" / "arch"
STATE_FILE = STATE_DIR / "outputs.json"


@dataclass
class TaskProgress:
    percent: float
    completed: int
    total: int


def load_manifest() -> dict:
    if not MANIFEST_PATH.exists():
        raise FileNotFoundError(f"Manifest not found: {MANIFEST_PATH}")
    with MANIFEST_PATH.open("r", encoding="utf-8") as fh:
        return yaml.safe_load(fh)


def ensure_json_serialisable(value: Any) -> Any:
    if isinstance(value, (str, int, float, bool)) or value is None:
        return value
    if isinstance(value, dt.datetime):
        if value.tzinfo:
            return value.astimezone(dt.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")
        return value.replace(microsecond=0).isoformat() + "Z"
    if isinstance(value, dict):
        return {key: ensure_json_serialisable(val) for key, val in value.items()}
    if isinstance(value, list):
        return [ensure_json_serialisable(item) for item in value]
    if isinstance(value, tuple):
        return [ensure_json_serialisable(item) for item in value]
    return str(value)


def load_state() -> Dict[str, str]:
    if not STATE_FILE.exists():
        return {}
    try:
        with STATE_FILE.open("r", encoding="utf-8") as fh:
            data = json.load(fh)
        if isinstance(data, dict):
            return {str(key): str(value) for key, value in data.items()}
    except json.JSONDecodeError:
        return {}
    return {}


def save_state(state: Dict[str, str]) -> None:
    STATE_DIR.mkdir(parents=True, exist_ok=True)
    with STATE_FILE.open("w", encoding="utf-8") as fh:
        json.dump(state, fh, ensure_ascii=False, indent=2)


def compute_task_progress(tasks: List[dict]) -> TaskProgress:
    if not tasks:
        return TaskProgress(percent=0.0, completed=0, total=0)
    total = len(tasks)
    completed = sum(1 for task in tasks if task["status"] == "done")
    percent = float(weighted_status_average(tasks, "status", "size_points"))
    return TaskProgress(percent=percent, completed=completed, total=total)


def organise_entities(manifest: dict) -> Tuple[Dict[str, dict], Dict[str, dict], Dict[str, dict]]:
    tasks = {task["id"]: task for task in manifest.get("tasks", [])}
    big_tasks = {big["id"]: big for big in manifest.get("big_tasks", [])}
    epics = {epic["id"]: epic for epic in manifest.get("epics", [])}
    return tasks, big_tasks, epics


def enrich_manifest(manifest: dict) -> dict:
    tasks, big_tasks, epics = organise_entities(manifest)

    for big_id, big in big_tasks.items():
        big_task_tasks = [task for task in tasks.values() if task["big_task"] == big_id]
        progress = compute_task_progress(big_task_tasks)
        big.setdefault("metrics", {})["progress_pct"] = int(round(progress.percent))
        big["stats"] = {"done": progress.completed, "total": progress.total}

    for epic_id, epic in epics.items():
        relevant_big_tasks = [big for big in big_tasks.values() if big["parent_epic"] == epic_id]
        if relevant_big_tasks:
            epic_progress = weighted_numeric_average(
                (
                    {
                        "value": big["metrics"]["progress_pct"],
                        "size_points": big.get("size_points", 1),
                    }
                    for big in relevant_big_tasks
                ),
                "value",
                "size_points",
            )
        else:
            epic_progress = int(round(status_score(epic["status"]) * 100))
        epic.setdefault("metrics", {})["progress_pct"] = epic_progress

    program = manifest.setdefault("program", {})
    meta = program.get("meta", {})
    epics_list = list(epics.values())
    if epics_list:
        program_progress = weighted_numeric_average(
            (
                {
                    "value": epic["metrics"]["progress_pct"],
                    "size_points": epic.get("size_points", 0),
                }
                for epic in epics_list
            ),
            "value",
            "size_points",
        )
    else:
        program_progress = 0.0
    program.setdefault("progress", {})["progress_pct"] = program_progress
    program.setdefault("progress", {}).setdefault("health", "green")
    phase_map = compute_phase_progress(manifest.get("tasks", []), program.get("milestones", []), program_progress)
    program.setdefault("progress", {})["phase_progress"] = phase_map
    milestones = program.get("milestones", [])
    for milestone in milestones:
        title = milestone.get("title")
        phase_value = phase_map.get(title, program_progress)
        milestone["status"] = status_from_progress(int(phase_value))
    meta.setdefault("updated_at", manifest.get("updated_at"))
    manifest["tasks_map"] = tasks
    manifest["big_tasks_map"] = big_tasks
    manifest["epics_map"] = epics
    return manifest


def render_program_section(manifest: dict) -> str:
    program_meta = manifest["program"]["meta"].copy()
    program_progress = manifest["program"]["progress"].copy()
    milestones = manifest["program"].get("milestones", [])

    program_block = program_meta | program_progress
    program_block["phase_progress"] = manifest["program"]["progress"].get("phase_progress", program_progress.get("phase_progress", {}))
    program_block["milestones"] = milestones
    program_block = ensure_json_serialisable(program_block)

    yaml_dump = yaml.dump(program_block, sort_keys=False, allow_unicode=True).strip()
    lines = ["## Program", "```yaml", yaml_dump, "```", "", "## Epics"]
    epics_data = []
    for epic in manifest["epics_map"].values():
        epic_block = {
            "id": epic["id"],
            "title": epic["title"],
            "type": epic["type"],
            "status": epic["status"],
            "priority": epic["priority"],
            "size_points": epic["size_points"],
            "scope_paths": epic.get("scope_paths", []),
            "spec": epic.get("spec", ""),
            "budgets": epic.get("budgets", {}),
            "risks": epic.get("risks", []),
            "dependencies": epic.get("dependencies", []),
            "docs_updates": epic.get("docs_updates", []),
            "artifacts": epic.get("artifacts", []),
            "big_tasks_planned": [big["id"] for big in manifest["big_tasks_map"].values() if big["parent_epic"] == epic["id"]],
            "progress_pct": epic["metrics"]["progress_pct"],
            "health": epic.get("health", "green"),
            "tests_required": epic.get("tests_required", []),
            "verify_commands": epic.get("verify_commands", []),
            "docs_updates": epic.get("docs_updates", []),
            "artifacts": epic.get("artifacts", []),
            "audit": epic.get("audit", {}),
        }
        epics_data.append(epic_block)
    epics_dump = yaml.dump(ensure_json_serialisable(epics_data), sort_keys=False, allow_unicode=True).strip()
    lines.extend(["```yaml", epics_dump, "```", "", "## Big Tasks"])

    big_tasks_data = []
    for big in manifest["big_tasks_map"].values():
        big_block = {
            "id": big["id"],
            "title": big["title"],
            "type": big["type"],
            "status": big["status"],
            "priority": big["priority"],
            "size_points": big["size_points"],
            "parent_epic": big["parent_epic"],
            "scope_paths": big.get("scope_paths", []),
            "spec": big.get("spec", ""),
            "budgets": big.get("budgets", {}),
            "risks": big.get("risks", []),
            "dependencies": big.get("dependencies", []),
            "progress_pct": big["metrics"]["progress_pct"],
            "health": big.get("health", "green"),
            "acceptance": big.get("acceptance", []),
            "tests_required": big.get("tests_required", []),
            "verify_commands": big.get("verify_commands", []),
            "docs_updates": big.get("docs_updates", []),
            "artifacts": big.get("artifacts", []),
            "audit": big.get("audit", {}),
        }
        big_tasks_data.append(big_block)
    big_dump = yaml.dump(ensure_json_serialisable(big_tasks_data), sort_keys=False, allow_unicode=True).strip()
    lines.extend(["```yaml", big_dump, "```", ""])
    return "\n".join(lines)


def render_tasks_board(manifest: dict) -> str:
    tasks = []
    manifest_tasks = manifest["tasks_map"]
    manifest_big_tasks = manifest["big_tasks_map"]
    for task in manifest_tasks.values():
        big = manifest_big_tasks[task["big_task"]]
        record = {
            "id": task["id"],
            "title": task["title"],
            "epic": big["parent_epic"],
            "status": task["status"],
            "priority": task["priority"],
            "owner": task["owner"],
            "success_criteria": task.get("success_criteria", []),
            "failure_criteria": task.get("failure_criteria", []),
            "blockers": task.get("blockers", []),
            "dependencies": task.get("dependencies", []),
            "conflicts": task.get("conflicts", []),
            "comments": task.get("comments", []),
            "size_points": task["size_points"],
            "big_task": task["big_task"],
            "system": task["system"],
            "roadmap_phase": task.get("roadmap_phase"),
            "metrics": task.get("metrics", {}),
        }
        tasks.append(record)
    tasks.sort(key=lambda item: item["id"])
    board = {
        "version": manifest.get("version", "v1"),
        "updated_at": manifest.get("updated_at"),
        "tasks": tasks,
    }
    return json.dumps(ensure_json_serialisable(board), ensure_ascii=False, indent=2) + "\n"


def render_architecture_overview(manifest: dict) -> str:
    program = manifest["program"]
    systems = manifest.get("systems", [])
    big_tasks = manifest["big_tasks_map"]
    tasks = manifest["tasks_map"]

    lines = [
        "# Architecture Overview",
        "",
        "## Program Snapshot",
        f"- Program ID: {program['meta']['program_id']}",
        f"- Name: {program['meta']['name']}",
        f"- Version: {manifest['version']}",
        f"- Updated: {ensure_json_serialisable(manifest['updated_at'])}",
        f"- Progress: {program['progress']['progress_pct']}% (health: {program['progress']['health']})",
        "",
        "## Systems",
        "| ID | Name | Purpose | ADR | RFC | Status | Dependencies | Roadmap Phase | Key Metrics |",
        "| --- | --- | --- | --- | --- | --- | --- | --- | --- |",
    ]
    for system in systems:
        deps = ", ".join(system.get("dependencies", [])) or "—"
        metrics = ", ".join(f"{k}={v}" for k, v in system.get("metrics", {}).items()) or "—"
        rfc = system.get("rfc") or "—"
        line = f"| {system['id']} | {system['name']} | {system['purpose']} | {system['adr']} | {rfc} | {system['status']} | {deps} | {system['roadmap_phase']} | {metrics} |"
        lines.append(line)
    lines.extend([
        "",
        "## Traceability",
        "| Task ID | Title | Status | Owner | System | Big Task | Epic | Phase |",
        "| --- | --- | --- | --- | --- | --- | --- | --- |",
    ])
    for task in tasks.values():
        big = big_tasks[task["big_task"]]
        row = "| {id} | {title} | {status} | {owner} | {system} | {big_task} | {epic} | {phase} |".format(
            id=task["id"],
            title=task["title"],
            status=task["status"],
            owner=task["owner"],
            system=task["system"],
            big_task=task["big_task"],
            epic=big["parent_epic"],
            phase=task.get("roadmap_phase", "—"),
        )
        lines.append(row)
    lines.extend([
        "",
        "## Documents",
        "- ADR Index: docs/adr/index.md",
        "- RFC Index: docs/rfc/index.md",
        "- Manifest: architecture/manifest.yaml",
    ])
    return "\n".join(lines) + "\n"


def render_adr_files(manifest: dict) -> Dict[str, str]:
    outputs: Dict[str, str] = {}
    adr_entries = manifest.get("adr", [])
    lines = ["# Architecture Decision Record Index", "", "| ADR | Title | Status | Date | Systems |", "| --- | --- | --- | --- | --- |"]
    for adr in adr_entries:
        systems = ", ".join(adr.get("related_systems", [])) or "—"
        lines.append(f"| {adr['id']} | {adr['title']} | {adr['status']} | {adr['date']} | {systems} |")
        content = "\n".join([
            f"# {adr['id']} — {adr['title']}",
            "",
            f"**Status:** {adr['status']} (date: {adr['date']})",
            f"**Authors:** {', '.join(adr.get('authors', [])) or '—'}",
            "",
            "## Context",
            adr.get("context", ""),
            "",
            "## Decision",
            adr.get("decision", ""),
            "",
            "## Consequences",
            adr.get("consequences", ""),
            "",
            f"**Related Systems:** {', '.join(adr.get('related_systems', [])) or '—'}",
            f"**Supersedes:** {', '.join(adr.get('supersedes', [])) or '—'}",
            f"**Superseded by:** {', '.join(adr.get('superseded_by', [])) or '—'}",
            "",
        ])
        outputs[f"docs/adr/{adr['id']}.md"] = content
    outputs["docs/adr/index.md"] = "\n".join(lines) + "\n"
    return outputs


def render_rfc_files(manifest: dict) -> Dict[str, str]:
    outputs: Dict[str, str] = {}
    rfc_entries = manifest.get("rfc", [])
    lines = ["# Request for Comments Index", "", "| RFC | Title | Status | Date | Systems |", "| --- | --- | --- | --- | --- |"]
    for rfc in rfc_entries:
        systems = ", ".join(rfc.get("related_systems", [])) or "—"
        lines.append(f"| {rfc['id']} | {rfc['title']} | {rfc['status']} | {rfc['date']} | {systems} |")
        content = "\n".join([
            f"# {rfc['id']} — {rfc['title']}",
            "",
            f"**Status:** {rfc['status']} (date: {rfc['date']})",
            f"**Authors:** {', '.join(rfc.get('authors', [])) or '—'}",
            "",
            "## Summary",
            rfc.get("summary", ""),
            "",
            "## Motivation",
            rfc.get("motivation", ""),
            "",
            "## Proposal",
            rfc.get("proposal", ""),
            "",
            f"**Related Systems:** {', '.join(rfc.get('related_systems', [])) or '—'}",
            f"**References:** {', '.join(rfc.get('references', [])) or '—'}",
            "",
        ])
        outputs[f"docs/rfc/{rfc['id']}.md"] = content
    outputs["docs/rfc/index.md"] = "\n".join(lines) + "\n"
    return outputs


def render_dashboard(manifest: dict) -> str:
    systems = manifest.get("systems", [])
    epics = manifest["epics_map"]
    big_tasks = manifest["big_tasks_map"]
    tasks = manifest["tasks_map"]
    generated_marker = manifest.get("updated_at") or dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")
    dashboard = {
        "generated_at": generated_marker,
        "manifest_version": manifest["version"],
        "manifest_updated_at": manifest["updated_at"],
        "program_progress_pct": manifest["program"]["progress"]["progress_pct"],
        "systems": systems,
        "epics": list(epics.values()),
        "big_tasks": list(big_tasks.values()),
        "tasks": list(tasks.values()),
    }
    return json.dumps(ensure_json_serialisable(dashboard), ensure_ascii=False, indent=2) + "\n"


def generate_outputs(manifest: dict) -> Dict[str, str]:
    manifest = enrich_manifest(manifest)
    outputs: Dict[str, str] = {}
    outputs["todo.machine.md"] = render_program_section(manifest)
    outputs["data/tasks.board.json"] = render_tasks_board(manifest)
    outputs["docs/architecture/overview.md"] = render_architecture_overview(manifest)
    outputs.update(render_adr_files(manifest))
    outputs.update(render_rfc_files(manifest))
    outputs["reports/architecture-dashboard.json"] = render_dashboard(manifest)
    return outputs


def compute_hash(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def write_if_changed(path: Path, content: str) -> bool:
    if path.exists():
        existing = path.read_text(encoding="utf-8")
        if existing == content:
            return False
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")
    return True


def sync_outputs() -> None:
    manifest = load_manifest()
    outputs = generate_outputs(manifest)
    state = load_state()
    force = os.getenv("ARCH_TOOL_FORCE") == "1"

    updated: List[str] = []
    skipped: List[str] = []
    new_state: Dict[str, str] = {}

    for rel_path, content in outputs.items():
        target = ROOT / rel_path
        recorded = rel_path in state

        if not force and not recorded and target.exists():
            existing = target.read_text(encoding="utf-8")
            if existing != content:
                skipped.append(rel_path)
                continue
            # пользовательский файл: не трогаем, не берём под управление
            continue

        changed = write_if_changed(target, content)
        if changed:
            updated.append(rel_path)

        if changed or recorded or force:
            new_state[rel_path] = compute_hash(content)

    if new_state:
        save_state(new_state)
    else:
        # Ensure stale state is cleared if nothing is managed anymore
        if STATE_FILE.exists():
            STATE_FILE.unlink()

    if updated:
        print("Updated:")
        for path in updated:
            print(f"  - {path}")
    else:
        print("All managed artifacts are up-to-date.")

    unmanaged = sorted(set(skipped))
    if unmanaged:
        print("Skipped user-managed files (use ARCH_TOOL_FORCE=1 to adopt):")
        for path in unmanaged:
            print(f"  - {path}")


def check_outputs() -> None:
    state = load_state()
    if not state:
        print("Architecture integrity confirmed (no managed artifacts).")
        return

    manifest = load_manifest()
    outputs = generate_outputs(manifest)
    mismatches: List[Tuple[str, str]] = []

    for rel_path, _hash in state.items():
        target = ROOT / rel_path
        content = outputs.get(rel_path)
        if content is None:
            mismatches.append((rel_path, "not produced by manifest"))
            continue
        if not target.exists():
            mismatches.append((rel_path, "missing"))
            continue
        existing = target.read_text(encoding="utf-8")
        if existing != content:
            mismatches.append((rel_path, "outdated"))

    if mismatches:
        print("Architecture integrity check failed:")
        for rel_path, reason in mismatches:
            print(f"  - {rel_path}: {reason}")
        print("Run `ARCH_TOOL_FORCE=1 agentcall run architecture-sync` to adopt generated artifacts or edit .sdk/arch/state.json.")
        sys.exit(1)

    print("Architecture integrity confirmed.")


def main(argv: List[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Architecture manifest tooling")
    subparsers = parser.add_subparsers(dest="command", required=True)
    subparsers.add_parser("sync", help="Generate artifacts from manifest")
    subparsers.add_parser("check", help="Validate generated artifacts")
    args = parser.parse_args(argv)
    if args.command == "sync":
        sync_outputs()
    else:
        check_outputs()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

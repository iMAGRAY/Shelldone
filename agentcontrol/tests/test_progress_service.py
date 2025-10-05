"""Tests for ProgressProjectionService using a temporary capsule."""
from __future__ import annotations

import json
from datetime import datetime, timezone
from pathlib import Path
from textwrap import dedent

import yaml

from agentcontrol.adapters.progress import (
    FileManifestRepository,
    FileStatusSnapshotRepository,
    FileTaskBoardRepository,
    FileTodoRepository,
)
from agentcontrol.app.progress.service import ProgressProjectionService


def _write_manifest(path: Path) -> None:
    data = {
        "program": {
            "meta": {"program_id": "demo", "name": "Demo Program"},
            "progress": {"progress_pct": 0, "health": "yellow"},
            "milestones": [
                {"title": "Phase Alpha", "status": "planned"},
                {"title": "Phase Beta", "status": "planned"},
            ],
        },
        "epics": [
            {"id": "epic-alpha", "title": "Epic Alpha", "status": "planned", "size_points": 8},
        ],
        "big_tasks": [
            {
                "id": "bt-alpha",
                "title": "Big Alpha",
                "parent_epic": "epic-alpha",
                "status": "planned",
                "size_points": 8,
                "roadmap_phase": "Phase Alpha",
            }
        ],
    }
    path.write_text(yaml.safe_dump(data, sort_keys=False), encoding="utf-8")


def _write_board(path: Path) -> None:
    board = {
        "version": "0.1",
        "updated_at": datetime.now(timezone.utc).isoformat(),
        "tasks": [
            {
                "id": "task-alpha",
                "status": "in_progress",
                "size_points": 8,
                "big_task": "bt-alpha",
                "epic": "epic-alpha",
            }
        ],
        "events": [],
        "assignments": {},
    }
    path.write_text(json.dumps(board, ensure_ascii=False, indent=2), encoding="utf-8")


def _write_todo(path: Path) -> None:
    path.write_text(
        dedent(
            """
            ## Program
            ```yaml
            progress_pct: 0
            health: yellow
            phase_progress: {}
            milestones: []
            updated_at: null
            ```

            ## Epics
            ```yaml
            - id: epic-alpha
              title: Epic Alpha
              progress_pct: 0
              status: planned
              health: yellow
            ```

            ## Big Tasks
            ```yaml
            - id: bt-alpha
              title: Big Alpha
              progress_pct: 0
              status: planned
              health: yellow
              parent_epic: epic-alpha
            ```
            """
        ).strip()
        + "\n",
        encoding="utf-8",
    )


def _build_service(tmp_path: Path) -> ProgressProjectionService:
    capsule = tmp_path / "agentcontrol"
    capsule.mkdir()
    (capsule / "architecture").mkdir()
    (capsule / "data").mkdir()
    (capsule / "reports").mkdir()

    manifest_path = capsule / "architecture" / "manifest.yaml"
    board_path = capsule / "data" / "tasks.board.json"
    todo_path = tmp_path / "todo.machine.md"
    status_path = capsule / "reports" / "status.json"

    _write_manifest(manifest_path)
    _write_board(board_path)
    _write_todo(todo_path)

    return ProgressProjectionService(
        manifest_repo=FileManifestRepository(manifest_path),
        task_board_repo=FileTaskBoardRepository(board_path),
        todo_repo=FileTodoRepository(todo_path),
        status_repo=FileStatusSnapshotRepository(status_path),
    )


def test_service_projection_updates_manifest(tmp_path: Path) -> None:
    service = _build_service(tmp_path)

    aggregate = service.compute()
    assert aggregate.computed_progress.value == 50

    manifest_projection = service.build_manifest_projection()
    program_progress = manifest_projection["program"]["progress"]["progress_pct"]
    assert program_progress == aggregate.computed_progress.value

    milestone_statuses = [item["status"] for item in manifest_projection["program"]["milestones"]]
    assert milestone_statuses[0] == "in_progress"

    service.save_manifest(manifest_projection)
    saved_manifest = yaml.safe_load((tmp_path / "agentcontrol" / "architecture" / "manifest.yaml").read_text(encoding="utf-8"))
    assert saved_manifest["program"]["progress"]["progress_pct"] == 50


def test_service_builds_status_snapshot(tmp_path: Path) -> None:
    service = _build_service(tmp_path)
    service.compute()

    status_payload = service.build_status_payload()
    assert status_payload["roadmap"]["program"]["progress_pct"] == 50
    assert status_payload["tasks"]["counts"]["in_progress"] == 1

    service.save_status(status_payload)
    saved = json.loads((tmp_path / "agentcontrol" / "reports" / "status.json").read_text(encoding="utf-8"))
    assert saved["roadmap"]["program"]["progress_pct"] == 50

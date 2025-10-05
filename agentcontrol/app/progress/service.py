"""Application service that orchestrates progress recomputation."""
from __future__ import annotations

from collections import defaultdict
from copy import deepcopy
from datetime import datetime, timezone
from pathlib import Path
from typing import Dict, MutableMapping, Optional

from agentcontrol.adapters.progress import (
    FileManifestRepository,
    FileStatusSnapshotRepository,
    FileTaskBoardRepository,
    FileTodoRepository,
)
from agentcontrol.domain.progress import ProgramProgressAggregate
from agentcontrol.ports.progress.repo_port import (
    ManifestRepository,
    StatusSnapshotRepository,
    TaskBoardRepository,
    TodoRepository,
)


class ProgressProjectionService:
    """Coordinates data loading, projection, and persistence."""

    def __init__(
        self,
        manifest_repo: ManifestRepository,
        task_board_repo: TaskBoardRepository,
        todo_repo: TodoRepository,
        status_repo: StatusSnapshotRepository,
    ) -> None:
        self._manifest_repo = manifest_repo
        self._task_board_repo = task_board_repo
        self._todo_repo = todo_repo
        self._status_repo = status_repo
        self._manifest: Optional[dict] = None
        self._task_board: Optional[dict] = None
        self._aggregate: Optional[ProgramProgressAggregate] = None

    @classmethod
    def default(cls, sdk_root: str) -> "ProgressProjectionService":
        root = Path(sdk_root)
        agent_dir = root / "agentcontrol"
        return cls(
            manifest_repo=FileManifestRepository(agent_dir / "architecture" / "manifest.yaml"),
            task_board_repo=FileTaskBoardRepository(agent_dir / "data" / "tasks.board.json"),
            todo_repo=FileTodoRepository(root / "todo.machine.md"),
            status_repo=FileStatusSnapshotRepository(agent_dir / "reports" / "status.json"),
        )

    def compute(self) -> ProgramProgressAggregate:
        manifest = self._manifest_repo.load()
        task_board = self._task_board_repo.load()
        aggregate = ProgramProgressAggregate.from_sources(manifest, task_board)
        self._manifest = manifest
        self._task_board = task_board
        self._aggregate = aggregate
        return aggregate

    def current_manifest(self) -> dict:
        if self._manifest is None:
            raise RuntimeError("compute() must be called before accessing manifest")
        return self._manifest

    def task_board(self) -> dict:
        if self._task_board is None:
            raise RuntimeError("compute() must be called before accessing task board")
        return self._task_board

    def aggregate(self) -> ProgramProgressAggregate:
        if self._aggregate is None:
            raise RuntimeError("Progress snapshot is not available; call compute() first")
        return self._aggregate

    def build_manifest_projection(self) -> dict:
        aggregate = self.aggregate()
        manifest = deepcopy(self.current_manifest())

        program_block: MutableMapping[str, object] = manifest.setdefault("program", {})
        meta = program_block.setdefault("meta", {})
        progress = program_block.setdefault("progress", {})
        progress["progress_pct"] = aggregate.computed_progress.value
        phase_map = {title: value.value for title, value in aggregate.phase_progress.items()}
        progress["phase_progress"] = phase_map
        progress.setdefault("health", aggregate.health)

        milestones = program_block.setdefault("milestones", [])
        for milestone in milestones:
            title = milestone.get("title")
            phase_value = phase_map.get(title)
            if phase_value is None:
                continue
            milestone["progress_pct"] = phase_value
            if phase_value >= 100:
                milestone["status"] = "done"
            elif phase_value > 0:
                milestone["status"] = "in_progress"
            else:
                milestone["status"] = "planned"

        manifest["updated_at"] = datetime.now(timezone.utc).isoformat()
        meta["updated_at"] = manifest["updated_at"]

        epic_index = {epic["id"]: epic for epic in manifest.get("epics", [])}
        for epic_state in aggregate.epics:
            epic = epic_index.get(epic_state.epic_id)
            if not epic:
                continue
            epic["status"] = epic_state.status
            epic["progress_pct"] = epic_state.computed.value
            metrics = epic.setdefault("metrics", {})
            metrics["progress_pct"] = epic_state.computed.value

        big_index = {bt["id"]: bt for bt in manifest.get("big_tasks", [])}
        for big_state in aggregate.big_tasks:
            big = big_index.get(big_state.big_task_id)
            if not big:
                continue
            big["status"] = big_state.status
            big["progress_pct"] = big_state.computed.value
            metrics = big.setdefault("metrics", {})
            metrics["progress_pct"] = big_state.computed.value

        return manifest

    def board_counts(self) -> Dict[str, int]:
        tasks = self.task_board().get("tasks", [])
        counts: Dict[str, int] = defaultdict(int)
        for task in tasks:
            status = str(task.get("status", "backlog"))
            counts[status] += 1
        return dict(counts)

    def build_status_payload(self) -> dict:
        aggregate = self.aggregate()
        board_counts = self.board_counts()
        payload = {
            "generated_at": aggregate.generated_at.isoformat(),
            "roadmap": aggregate.to_serialisable(),
            "tasks": {
                "generated_at": aggregate.generated_at.isoformat(),
                "board_version": str(self.task_board().get("version", "0")),
                "updated_at": self.task_board().get("updated_at"),
                "counts": board_counts,
                "events": self.task_board().get("events", []),
                "assignments": self.task_board().get("assignments", {}),
            },
        }
        return payload

    def save_manifest(self, manifest: dict) -> None:
        self._manifest_repo.save(manifest)

    def load_todo_text(self) -> str:
        return self._todo_repo.load()

    def save_todo_text(self, content: str) -> None:
        self._todo_repo.save(content)

    def save_status(self, payload: dict) -> None:
        self._status_repo.save(payload)

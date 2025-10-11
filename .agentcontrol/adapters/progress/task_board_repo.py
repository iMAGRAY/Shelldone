"""JSON-backed task board repository."""
from __future__ import annotations

import json
from pathlib import Path
from typing import Any, Dict

from agentcontrol.ports.progress.repo_port import TaskBoardRepository


class FileTaskBoardRepository(TaskBoardRepository):
    def __init__(self, path: Path) -> None:
        self._path = path

    def load(self) -> Dict[str, Any]:
        if not self._path.exists():
            return {"tasks": []}
        with self._path.open("r", encoding="utf-8") as handle:
            data = json.load(handle)
        if not isinstance(data, dict):  # pragma: no cover - defensive guard
            raise ValueError("tasks.board.json must contain an object")
        data.setdefault("tasks", [])
        return data

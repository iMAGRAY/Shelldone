"""Status snapshot persistence."""
from __future__ import annotations

import json
from pathlib import Path
from typing import Any, Dict, List

from agentcontrol.ports.progress.repo_port import StatusSnapshotRepository


class FileStatusSnapshotRepository(StatusSnapshotRepository):
    def __init__(self, path: Path) -> None:
        self._path = path

    def save(self, payload: Dict[str, Any]) -> None:
        self._path.parent.mkdir(parents=True, exist_ok=True)
        with self._path.open("w", encoding="utf-8") as handle:
            json.dump(payload, handle, ensure_ascii=False, indent=2)
            handle.write("\n")

    def load_history(self) -> List[Dict[str, Any]]:
        if not self._path.exists():
            return []
        with self._path.open("r", encoding="utf-8") as handle:
            try:
                data = json.load(handle)
            except json.JSONDecodeError:  # pragma: no cover - defensive guard
                return []
        if isinstance(data, list):
            return data
        if isinstance(data, dict):
            return [data]
        return []

"""Plain-text repository for todo.machine.md."""
from __future__ import annotations

from pathlib import Path

from agentcontrol.ports.progress.repo_port import TodoRepository


class FileTodoRepository(TodoRepository):
    def __init__(self, path: Path) -> None:
        self._path = path

    def load(self) -> str:
        if not self._path.exists():
            raise FileNotFoundError(f"todo.machine.md not found at {self._path}")
        return self._path.read_text(encoding="utf-8")

    def save(self, content: str) -> None:
        self._path.parent.mkdir(parents=True, exist_ok=True)
        self._path.write_text(content, encoding="utf-8")

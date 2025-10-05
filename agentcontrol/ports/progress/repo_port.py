"""Repository ports for the progress bounded context."""
from __future__ import annotations

from typing import Protocol, Sequence


class ManifestRepository(Protocol):
    """Gateway for reading/writing architecture manifest data."""

    def load(self) -> dict:
        ...

    def save(self, manifest: dict) -> None:
        ...


class TaskBoardRepository(Protocol):
    """Access to the structured task board."""

    def load(self) -> dict:
        ...


class TodoRepository(Protocol):
    """Gateway for todo.machine.md structured sections."""

    def load(self) -> str:
        ...

    def save(self, content: str) -> None:
        ...


class StatusSnapshotRepository(Protocol):
    """Persists status snapshots for downstream tooling."""

    def save(self, payload: dict) -> None:
        ...

    def load_history(self) -> Sequence[dict]:  # pragma: no cover - optional capability
        ...

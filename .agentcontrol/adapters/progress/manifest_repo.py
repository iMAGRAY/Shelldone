"""YAML-backed manifest repository."""
from __future__ import annotations

from pathlib import Path
from typing import Any, Dict

import yaml

from agentcontrol.ports.progress.repo_port import ManifestRepository


class FileManifestRepository(ManifestRepository):
    """Read/write access to architecture/manifest.yaml."""

    def __init__(self, path: Path) -> None:
        self._path = path

    def load(self) -> Dict[str, Any]:
        if not self._path.exists():
            raise FileNotFoundError(f"Manifest not found at {self._path}")
        with self._path.open("r", encoding="utf-8") as handle:
            data = yaml.safe_load(handle) or {}
        if not isinstance(data, dict):  # pragma: no cover - defensive guard
            raise ValueError("manifest.yaml must contain a mapping")
        return data

    def save(self, manifest: Dict[str, Any]) -> None:
        self._path.parent.mkdir(parents=True, exist_ok=True)
        with self._path.open("w", encoding="utf-8") as handle:
            yaml.safe_dump(manifest, handle, sort_keys=False, allow_unicode=True, width=1000)

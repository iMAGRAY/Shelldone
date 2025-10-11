#!/usr/bin/env python3
"""
Generate a lightweight SBOM using cargo metadata.
"""
from __future__ import annotations

import hashlib
import json
import os
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from typing import Iterable, List


ROOT = Path(__file__).resolve().parent.parent
SBOM_DIR = ROOT / "reports" / "sbom"
SBOM_FILE = SBOM_DIR / "cargo.json"
FINGERPRINT_KEY = "_fingerprint"

EXCLUDE_DIRS = {
    ".git",
    "target",
    "artifacts",
    "reports",
    "__pycache__",
    ".venv",
}


def iter_cargo_manifests() -> Iterable[Path]:
    for path in ROOT.rglob("Cargo.toml"):
        if any(part in EXCLUDE_DIRS for part in path.parts):
            continue
        yield path


def fingerprint_sources() -> str:
    inputs: List[Path] = [ROOT / "Cargo.lock", ROOT / "Cargo.toml"]
    inputs.extend(iter_cargo_manifests())
    hasher = hashlib.sha256()
    for path in sorted({p for p in inputs if p.exists()}):
        hasher.update(path.relative_to(ROOT).as_posix().encode("utf-8"))
        hasher.update(path.read_bytes())
    env_hash = os.environ.get("CARGO_SBOM_ENV_HASH")
    if env_hash:
        hasher.update(env_hash.encode("utf-8"))
    return hasher.hexdigest()


def load_existing_fingerprint() -> str | None:
    if not SBOM_FILE.exists():
        return None
    try:
        payload = json.loads(SBOM_FILE.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return None
    return payload.get(FINGERPRINT_KEY)


def cargo_metadata() -> dict:
    result = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--all-features"],
        check=True,
        capture_output=True,
        text=True,
    )
    return json.loads(result.stdout)


def build_sbom(metadata: dict, fingerprint: str) -> dict:
    packages = [
        {
            "name": pkg.get("name"),
            "version": pkg.get("version"),
            "license": pkg.get("license"),
            "source": pkg.get("source"),
        }
        for pkg in metadata.get("packages", [])
    ]
    return {
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "workspace_root": metadata.get("workspace_root"),
        "workspace_members": metadata.get("workspace_members"),
        "package_count": len(packages),
        "packages": packages,
        FINGERPRINT_KEY: fingerprint,
    }


def main() -> None:
    SBOM_DIR.mkdir(parents=True, exist_ok=True)
    current_fingerprint = fingerprint_sources()
    if load_existing_fingerprint() == current_fingerprint:
        print(f"[sbom] up-to-date ({SBOM_FILE.relative_to(ROOT)}) fingerprint={current_fingerprint}")
        return

    metadata = cargo_metadata()
    sbom = build_sbom(metadata, current_fingerprint)
    SBOM_FILE.write_text(json.dumps(sbom, indent=2, ensure_ascii=False), encoding="utf-8")
    print(f"[sbom] wrote {SBOM_FILE.relative_to(ROOT)} ({sbom['package_count']} packages)")


if __name__ == "__main__":
    main()

"""Verify QA orchestrator exposes expected checks."""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

from scripts import verify as verify_module


def test_python_build_check_listed() -> None:
    root = Path(__file__).resolve().parents[2]
    script = root / "scripts" / "verify.py"
    completed = subprocess.run(
        [sys.executable, str(script), "--list-checks"],
        cwd=root,
        check=True,
        capture_output=True,
        text=True,
    )
    assert any(line.startswith("python-build:") for line in completed.stdout.splitlines())


def test_compute_python_build_fingerprint_changes(tmp_path: Path) -> None:
    file_a = tmp_path / "a.txt"
    file_b = tmp_path / "b.txt"
    file_a.write_text("alpha", encoding="utf-8")
    file_b.write_text("beta", encoding="utf-8")
    first = verify_module.compute_python_build_fingerprint([file_a, file_b])
    file_b.write_text("gamma", encoding="utf-8")
    second = verify_module.compute_python_build_fingerprint([file_a, file_b])
    assert first != second


def test_python_build_cache_validation(tmp_path: Path) -> None:
    input_file = tmp_path / "input.txt"
    input_file.write_text("payload", encoding="utf-8")
    fingerprint = verify_module.compute_python_build_fingerprint([input_file])

    dist_dir = tmp_path / "dist"
    dist_dir.mkdir()
    artifact = dist_dir / "pkg.whl"
    artifact.write_bytes(b"wheel-data")

    manifest = {
        "fingerprint": fingerprint,
        "files": [
            {
                "path": artifact.relative_to(tmp_path).as_posix(),
                "sha256": verify_module._hash_file(artifact),
            }
        ],
        "env_signature": verify_module.python_build_env_signature().hex(),
    }

    assert verify_module.is_python_build_cache_valid(fingerprint, manifest, tmp_path)

    artifact.write_bytes(b"corrupted")
    assert not verify_module.is_python_build_cache_valid(fingerprint, manifest, tmp_path)

    input_file.write_text("payload-updated", encoding="utf-8")
    new_fingerprint = verify_module.compute_python_build_fingerprint([input_file])
    assert not verify_module.is_python_build_cache_valid(new_fingerprint, manifest, tmp_path)

    manifest["env_signature"] = "deadbeef"
    assert not verify_module.is_python_build_cache_valid(fingerprint, manifest, tmp_path)


def test_compute_python_build_fingerprint_skips_missing(tmp_path: Path) -> None:
    existing = tmp_path / "live.txt"
    missing = tmp_path / "missing.txt"
    existing.write_text("seed", encoding="utf-8")
    fingerprint = verify_module.compute_python_build_fingerprint([existing, missing])
    assert fingerprint  # non-empty


def test_compute_python_build_fingerprint_depends_on_env_signature(monkeypatch, tmp_path: Path) -> None:
    sample = tmp_path / "file.txt"
    sample.write_text("content", encoding="utf-8")
    baseline = verify_module.compute_python_build_fingerprint([sample])
    monkeypatch.setattr(verify_module, "python_build_env_signature", lambda: b"custom-env")
    shifted = verify_module.compute_python_build_fingerprint([sample])
    assert baseline != shifted


def test_resolve_virtualenv_tool_prefers_capsule(monkeypatch, tmp_path: Path) -> None:
    capsule_root = tmp_path / "capsule"
    capsule_bin = capsule_root / ".venv" / "bin"
    capsule_bin.mkdir(parents=True)
    capsule_python = capsule_bin / "python"
    capsule_python.write_text("", encoding="utf-8")

    repo_root = tmp_path / "repo"
    repo_bin = repo_root / ".venv" / "bin"
    repo_bin.mkdir(parents=True)
    repo_python = repo_bin / "python"
    repo_python.write_text("", encoding="utf-8")

    monkeypatch.setattr(verify_module, "CAPSULE_ROOT", capsule_root)
    monkeypatch.setattr(verify_module, "ROOT", repo_root)

    resolved = verify_module.resolve_virtualenv_tool("python")
    assert resolved == capsule_python.resolve()


def test_resolve_virtualenv_tool_falls_back(monkeypatch, tmp_path: Path) -> None:
    capsule_root = tmp_path / "capsule-missing"
    repo_root = tmp_path / "repo"
    repo_bin = repo_root / ".venv" / "bin"
    repo_bin.mkdir(parents=True)
    repo_pytest = repo_bin / "pytest"
    repo_pytest.write_text("", encoding="utf-8")

    monkeypatch.setattr(verify_module, "CAPSULE_ROOT", capsule_root)
    monkeypatch.setattr(verify_module, "ROOT", repo_root)

    resolved = verify_module.resolve_virtualenv_tool("pytest")
    assert resolved == repo_pytest.resolve()

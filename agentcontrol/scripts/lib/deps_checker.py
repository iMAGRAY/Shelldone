#!/usr/bin/env python3
"""Проверка окружения и зависимостей для GPT-5 Codex SDK."""

from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import sys
from dataclasses import dataclass, asdict
from datetime import datetime, timezone
from importlib import metadata as importlib_metadata
from pathlib import Path
from typing import Iterable, Optional


@dataclass(slots=True)
class CheckResult:
    name: str
    status: str
    details: str
    fix: str


ROOT = Path(__file__).resolve().parents[2]
VENV_DIR = ROOT / ".venv"
VENV_BIN = VENV_DIR / ("Scripts" if os.name == "nt" else "bin")
VENV_PYTHON = VENV_BIN / ("python.exe" if os.name == "nt" else "python")


def which(cmd: str) -> Optional[str]:
    path = shutil.which(cmd)
    if path:
        return path
    candidate = VENV_BIN / cmd
    if candidate.exists():
        return str(candidate)
    if os.name == "nt":
        candidate_exe = candidate.with_suffix(".exe")
        if candidate_exe.exists():
            return str(candidate_exe)
    return None


def _version_string(package: str) -> tuple[bool, str]:
    if VENV_PYTHON.exists():
        ok, out = run([str(VENV_PYTHON), "-c", f"import importlib.metadata as m; print(m.version('{package}'))"])
        if ok and out.strip():
            return True, out.strip()
    try:
        return True, importlib_metadata.version(package)
    except importlib_metadata.PackageNotFoundError:
        return False, ""


def _version_tuple(version: str) -> tuple[int, ...]:
    digits = re.findall(r"\d+", version)
    return tuple(int(d) for d in digits) if digits else (0,)


def module_available(module: str, package: str, minimum: Optional[str]) -> tuple[str, str]:
    if VENV_PYTHON.exists():
        ok, _ = run([str(VENV_PYTHON), "-c", f"import {module}"])
        if not ok:
            return "missing", ""
        origin = str(VENV_PYTHON)
    else:
        try:
            __import__(module)
        except ImportError:
            return "missing", ""
        origin = "system"

    has_version, version = _version_string(package)
    if has_version and minimum:
        if _version_tuple(version) < _version_tuple(minimum):
            return "outdated", f"{version} (<{minimum})"
    details = version or origin
    return "ok", details


def run(cmd: list[str]) -> tuple[bool, str]:
    try:
        out = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=True, text=True)
        return True, out.stdout.strip()
    except (OSError, subprocess.CalledProcessError) as exc:  # pragma: no cover - depends on env
        return False, str(exc)


def detect_python_packages() -> Iterable[CheckResult]:
    requirements = (
        ("pytest", "pytest", "8.4.2"),
        ("diff_cover", "diff_cover", "9.7.1"),
        ("detect_secrets", "detect_secrets", "1.5.0"),
    )
    for pkg_name, module, minimum in requirements:
        status, details = module_available(module, pkg_name.replace("_", "-"), minimum)
        if status == "missing":
            fix = f"pip install {pkg_name.replace('_', '-')}"
        elif status == "outdated":
            fix = f"pip install -U {pkg_name.replace('_', '-')}"
        else:
            fix = ""
        yield CheckResult(name=f"python:{pkg_name}", status=status, details=details, fix=fix)


def detect_tools() -> Iterable[CheckResult]:
    commands = {
        "git": "apt install git",
        "make": "apt install make",
        "bash": "already required",
        "shellcheck": "apt install shellcheck",
        "diff-cover": "pip install diff-cover",
        "detect-secrets": "pip install detect-secrets",
        "reviewdog": "GOBIN=$PWD/.venv/bin go install github.com/reviewdog/reviewdog/cmd/reviewdog@v0.15.0",
        "go": "apt install golang-go",
    }
    for cmd, fix in commands.items():
        location = which(cmd)
        if location:
            status, details = "ok", location
        else:
            status, details = "missing", ""
        yield CheckResult(name=f"tool:{cmd}", status=status, details=details, fix=fix)


def detect_stack_configs(root: Path) -> Iterable[CheckResult]:
    entries = {
        "package.json": "npm install",
        "yarn.lock": "yarn install",
        "pnpm-lock.yaml": "pnpm install",
        "Pipfile": "pipenv install --dev",
        "poetry.lock": "poetry install",
        "go.mod": "go mod download",
        "Cargo.toml": "cargo fetch",
        "pom.xml": "mvn -B verify",
        "build.gradle": "./gradlew check",
        "build.gradle.kts": "./gradlew check",
        "requirements.txt": "pip install -r requirements.txt",
    }
    for rel, tip in entries.items():
        if (root / rel).exists():
            yield CheckResult(name=f"stack:{rel}", status="detected", details=tip, fix=tip)


def collect(root: Path) -> dict:
    results = [
        *detect_tools(),
        *detect_python_packages(),
        *detect_stack_configs(root),
    ]
    summary = {
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "root": str(root),
        "results": [asdict(r) for r in results],
    }
    return summary


def main(argv: list[str] | None = None) -> int:
    argv = argv or sys.argv[1:]
    root = Path(argv[0]).resolve() if argv else Path.cwd()
    report = collect(root)
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())

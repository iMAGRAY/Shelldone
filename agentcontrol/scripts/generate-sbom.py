#!/usr/bin/env python3
"""Generate or verify combined SBOM (Python + system + Go)."""
from __future__ import annotations

import argparse
import json
import shutil
import subprocess
from dataclasses import asdict, dataclass
from importlib import metadata as importlib_metadata
from pathlib import Path
from typing import Dict, List, Optional


@dataclass
class Package:
    name: str
    version: str
    summary: str


def collect_packages() -> List[Package]:
    packages: List[Package] = []
    for dist in importlib_metadata.distributions():
        name = dist.metadata.get("Name")
        if not name:
            name = dist.metadata.get("Summary") or dist.metadata.get("Author") or dist.metadata.get("Generator") or dist.metadata.get("Home-page") or "unknown"
        version = dist.version or "0"
        summary = dist.metadata.get("Summary", "")
        packages.append(Package(name=name, version=version, summary=summary))
    packages.sort(key=lambda pkg: pkg.name.lower())
    return packages


def to_json(packages: List[Package]) -> str:
    return json.dumps([asdict(pkg) for pkg in packages], ensure_ascii=False, indent=2) + "\n"


SYSTEM_PACKAGES = (
    "shellcheck",
    "golang-go",
    "python3-venv",
    "python3-pip",
)


def system_version(pkg: str) -> Optional[str]:
    if not shutil.which("dpkg-query"):
        return None
    try:
        result = subprocess.run(
            ["dpkg-query", "-W", "-f=${Version}", pkg],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
        )
    except subprocess.CalledProcessError:
        return None
    return result.stdout.strip() or None


def collect_system_packages() -> List[Dict[str, Optional[str]]]:
    system_entries: List[Dict[str, Optional[str]]] = []
    for pkg in SYSTEM_PACKAGES:
        system_entries.append({
            "name": pkg,
            "version": system_version(pkg),
        })
    return system_entries


def run_command(cmd: List[str]) -> Optional[str]:
    try:
        out = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, check=True, text=True)
    except (OSError, subprocess.CalledProcessError):
        return None
    return out.stdout.strip()


def collect_go_info() -> Dict[str, Optional[str]]:
    info: Dict[str, Optional[str]] = {}
    go_bin = shutil.which("go")
    if go_bin:
        info["go"] = run_command([go_bin, "version"])
    reviewdog_bin = shutil.which("reviewdog")
    if reviewdog_bin:
        info["reviewdog"] = run_command([reviewdog_bin, "version"])
    else:
        venv_reviewdog = Path(__file__).resolve().parents[2] / ".venv" / "bin" / "reviewdog"
        if venv_reviewdog.exists():
            info["reviewdog"] = run_command([str(venv_reviewdog), "-version"])
    return info


def main() -> int:
    parser = argparse.ArgumentParser(description="Generate or verify Python SBOM")
    parser.add_argument("--output", default="sbom/python.json", help="Path to write SBOM JSON")
    parser.add_argument("--check", action="store_true", help="Verify existing SBOM matches current environment")
    args = parser.parse_args()

    packages = collect_packages()
    sbom = {
        "python": [asdict(pkg) for pkg in packages],
        "system": collect_system_packages(),
        "go": collect_go_info(),
    }
    sbom_json = json.dumps(sbom, ensure_ascii=False, indent=2) + "\n"
    output_path = Path(args.output)

    if args.check:
        if not output_path.exists():
            print(f"SBOM not found: {output_path}")
            return 1
        existing = output_path.read_text(encoding="utf-8")
        if existing != sbom_json:
            print("SBOM mismatch detected. Regenerate via scripts/update-lock.sh")
            return 1
        print("SBOM verified")
        return 0

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(sbom_json, encoding="utf-8")
    print(f"SBOM written to {output_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

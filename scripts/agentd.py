#!/usr/bin/env python3
"""Shelldone agent daemon utility.

Loads adapter definitions from `agents/manifest.json`, allows listing, running
and smoke-testing adapters. Intended as a lightweight helper until the full
`shelldone-agentd` service is implemented.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, List, Sequence

ROOT = Path(__file__).resolve().parent.parent
MANIFEST = ROOT / "agents" / "manifest.json"


@dataclass
class Adapter:
    adapter_id: str
    description: str
    command: List[str]
    error_contains: List[str]

    @property
    def runtime(self) -> str:
        return self.command[0]


class ManifestError(RuntimeError):
    """Raised when the manifest is missing or malformed."""


def _expand_command(raw_command: Sequence[str]) -> List[str]:
    expanded: List[str] = []
    for idx, piece in enumerate(raw_command):
        part = piece
        if part == "{python}":
            part = sys.executable
        elif part.startswith("{ROOT}/"):
            part = str(ROOT / part[len("{ROOT}/") :])
        elif idx > 0 and not os.path.isabs(part) and (
            part.endswith(".py")
            or part.endswith(".mjs")
            or part.endswith(".js")
            or part.endswith(".sh")
        ):
            part = str(ROOT / part)
        expanded.append(part)
    return expanded


def load_manifest() -> List[Adapter]:
    if not MANIFEST.exists():
        raise ManifestError(f"Manifest {MANIFEST} not found")
    data = json.loads(MANIFEST.read_text(encoding="utf-8"))
    adapters = []
    for item in data.get("adapters", []):
        adapter_id = item.get("id")
        command = item.get("command", [])
        description = item.get("description", "")
        error_contains = item.get("error_contains", [])
        if not adapter_id or not command:
            raise ManifestError("Invalid adapter entry in manifest")
        adapters.append(
            Adapter(
                adapter_id=adapter_id,
                description=description,
                command=_expand_command(command),
                error_contains=list(error_contains),
            )
        )
    return adapters


def list_adapters(adapters: Iterable[Adapter]) -> int:
    payload = {
        "adapters": [
            {
                "id": adapter.adapter_id,
                "description": adapter.description,
                "command": adapter.command,
                "runtime": adapter.runtime,
            }
            for adapter in adapters
        ]
    }
    print(json.dumps(payload, indent=2, ensure_ascii=False))
    return 0


def _runtime_available(adapter: Adapter) -> bool:
    runtime = adapter.runtime
    if os.path.isabs(runtime):
        return Path(runtime).exists()
    return shutil.which(runtime) is not None


def smoke_test_adapter(adapter: Adapter) -> tuple[bool, str]:
    if not _runtime_available(adapter):
        return True, f"SKIP {adapter.adapter_id}: runtime {adapter.runtime} not found"

    try:
        proc = subprocess.run(
            adapter.command,
            cwd=str(ROOT),
            capture_output=True,
            text=True,
            timeout=10,
        )
    except Exception as exc:  # pragma: no cover - defensive
        return False, f"{adapter.adapter_id}: failed to invoke adapter ({exc})"

    stdout_lines = proc.stdout.strip().splitlines()
    if not stdout_lines:
        return False, f"{adapter.adapter_id}: no output"
    try:
        payload = json.loads(stdout_lines[0])
    except json.JSONDecodeError as exc:
        return False, f"{adapter.adapter_id}: invalid JSON output ({exc})"

    if payload.get("status") != "error":
        return False, f"{adapter.adapter_id}: expected status=error, got {payload.get('status')}"

    error_text = str(payload.get("error", ""))
    if adapter.error_contains and not any(
        token in error_text for token in adapter.error_contains
    ):
        return False, f"{adapter.adapter_id}: unexpected error message: {error_text!r}"

    return True, f"OK {adapter.adapter_id}: {error_text or 'error emitted as expected'}"


def smoke_test(adapters: Iterable[Adapter]) -> int:
    statuses: List[str] = []
    success = True
    for adapter in adapters:
        ok, message = smoke_test_adapter(adapter)
        statuses.append(message)
        if not ok:
            success = False
    print("\n".join(statuses))
    return 0 if success else 1


def run_adapter(adapter: Adapter) -> int:
    if not _runtime_available(adapter):
        print(f"Runtime {adapter.runtime} not found", file=sys.stderr)
        return 1

    proc = subprocess.Popen(
        adapter.command,
        cwd=str(ROOT),
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        bufsize=1,
    )
    assert proc.stdout is not None
    assert proc.stdin is not None

    print(
        f"Adapter {adapter.adapter_id} started. Proxying STDIN → adapter → STDOUT. Ctrl-D to exit.",
        file=sys.stderr,
    )
    try:
        for line in sys.stdin:
            proc.stdin.write(line)
            proc.stdin.flush()
            response = proc.stdout.readline()
            if not response:
                break
            sys.stdout.write(response)
            sys.stdout.flush()
    except KeyboardInterrupt:
        pass
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
    return proc.returncode or 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Shelldone agent daemon helper")
    subparsers = parser.add_subparsers(dest="command")

    subparsers.add_parser("list", help="List available adapters")

    run_parser = subparsers.add_parser("run", help="Run adapter and proxy stdin/stdout")
    run_parser.add_argument("adapter_id")

    subparsers.add_parser("smoke", help="Run smoke tests for all adapters")
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    try:
        adapters = load_manifest()
    except ManifestError as exc:
        print(exc, file=sys.stderr)
        return 1

    adapter_map = {adapter.adapter_id: adapter for adapter in adapters}

    if args.command == "list":
        return list_adapters(adapters)
    if args.command == "run":
        adapter = adapter_map.get(args.adapter_id)
        if not adapter:
            print(f"Adapter {args.adapter_id!r} not found", file=sys.stderr)
            return 1
        return run_adapter(adapter)
    if args.command == "smoke":
        return smoke_test(adapters)

    parser.print_help()
    return 1


if __name__ == "__main__":
    sys.exit(main())

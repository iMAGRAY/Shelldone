#!/usr/bin/env python3
"""
Run TermBridge snapshot export with OTLP emission and validate payload.

This wraps the collector, matrix export, and payload checker so that
flagship smoke tests can be executed reproducibly.
"""

from __future__ import annotations

import argparse
import os
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import time
from datetime import datetime, timezone
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_ARTIFACT_DIR = (
    REPO_ROOT / "reports" / "roadmap" / "termbridge" / "termbridge-telemetry"
)
COLLECTOR_SCRIPT = REPO_ROOT / "scripts" / "tests" / "mock_otlp_collector.py"
MATRIX_SCRIPT = REPO_ROOT / "scripts" / "tests" / "termbridge_matrix.py"
CHECK_SCRIPT = REPO_ROOT / "scripts" / "tests" / "check_otlp_payload.py"


def _wait_for_port(port: int, timeout: float = 5.0) -> None:
    deadline = time.time() + timeout
    while time.time() < deadline:
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
            sock.settimeout(0.2)
            try:
                sock.connect(("127.0.0.1", port))
            except OSError:
                time.sleep(0.1)
                continue
        return
    raise RuntimeError(f"Collector on port {port} did not start within {timeout} seconds")


def run_smoke(port: int, artifact_dir: Path) -> tuple[Path, Path]:
    artifact_dir.mkdir(parents=True, exist_ok=True)
    timestamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    snapshot_tmp = Path(tempfile.mkstemp(prefix="termbridge_snapshot_", suffix=".json")[1])
    payload_tmp = Path(tempfile.mkstemp(prefix="termbridge_otlp_", suffix=".json")[1])

    collector_env = os.environ.copy()
    collector_cmd = [
        sys.executable,
        str(COLLECTOR_SCRIPT),
        "--port",
        str(port),
        "--output",
        str(payload_tmp),
    ]
    collector_proc = subprocess.Popen(
        collector_cmd,
        cwd=str(REPO_ROOT),
        env=collector_env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    try:
        _wait_for_port(port)
        matrix_cmd = [
            sys.executable,
            str(MATRIX_SCRIPT),
            "--emit-otlp",
            "--otlp-endpoint",
            f"http://127.0.0.1:{port}/v1/metrics",
            "--output",
            str(snapshot_tmp),
        ]
        subprocess.run(matrix_cmd, cwd=str(REPO_ROOT), check=True)
    finally:
        if collector_proc.poll() is None:
            collector_proc.send_signal(signal.SIGTERM)
            try:
                collector_proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                collector_proc.kill()
                collector_proc.wait()

    if collector_proc.returncode not in (0, -signal.SIGTERM):
        stdout, stderr = collector_proc.communicate()
        raise RuntimeError(
            f"Collector exited with {collector_proc.returncode}\nstdout:\n{stdout}\nstderr:\n{stderr}"
        )

    check_cmd = [
        sys.executable,
        str(CHECK_SCRIPT),
        "--payload",
        str(payload_tmp),
        "--snapshot",
        str(snapshot_tmp),
    ]
    subprocess.run(check_cmd, cwd=str(REPO_ROOT), check=True)

    snapshot_dst = artifact_dir / f"{timestamp}_snapshot.json"
    payload_dst = artifact_dir / f"{timestamp}_otlp.json"
    shutil.move(snapshot_tmp, snapshot_dst)
    shutil.move(payload_tmp, payload_dst)
    return snapshot_dst, payload_dst


def main() -> int:
    parser = argparse.ArgumentParser(description="TermBridge OTLP telemetry smoke test")
    parser.add_argument("--port", type=int, default=4346, help="OTLP collector port (default: 4346)")
    parser.add_argument(
        "--artifacts-dir",
        type=Path,
        default=DEFAULT_ARTIFACT_DIR,
        help=f"Directory for storing artifacts (default: {DEFAULT_ARTIFACT_DIR})",
    )
    args = parser.parse_args()
    snapshot_dst, payload_dst = run_smoke(args.port, args.artifacts_dir)
    print(f"Snapshot: {snapshot_dst}")
    print(f"OTLP payload: {payload_dst}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

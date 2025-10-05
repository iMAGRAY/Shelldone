from __future__ import annotations

import subprocess
import time
from pathlib import Path
from typing import Optional, Tuple
from urllib import error as urllib_error, request as urllib_request



def wait_for_agentd(listen: str, timeout: float = 30.0) -> None:
    url = f"http://{listen}/healthz"
    deadline = time.time() + timeout
    last_error: Optional[Exception] = None
    while time.time() < deadline:
        try:
            with urllib_request.urlopen(url, timeout=1) as response:
                if response.status == 200:
                    return
        except urllib_error.URLError as err:  # pragma: no cover - network dependent
            last_error = err
            time.sleep(0.25)
    raise RuntimeError(f"Timed out waiting for shelldone-agentd at {url}: {last_error}")


def start_agentd(
    *,
    state_dir: Path,
    log_path: Path,
    cwd: Path,
    env: dict,
    listen: str = "127.0.0.1:17717",
    grpc_listen: str = "127.0.0.1:17718",
) -> Tuple[subprocess.Popen, object]:
    command = [
        "cargo",
        "run",
        "-p",
        "shelldone-agentd",
        "--",
        "--listen",
        listen,
        "--grpc-listen",
        grpc_listen,
        "--state-dir",
        str(state_dir),
    ]
    log_path.parent.mkdir(parents=True, exist_ok=True)
    handle = log_path.open("w", encoding="utf-8")
    process = subprocess.Popen(
        command,
        cwd=str(cwd),
        env=env,
        stdout=handle,
        stderr=subprocess.STDOUT,
        text=True,
    )
    return process, handle


def stop_agentd(process: subprocess.Popen, handle: object) -> None:
    try:
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:  # pragma: no cover - unlikely
                process.kill()
                process.wait(timeout=5)
    finally:
        if handle is not None:
            handle.close()

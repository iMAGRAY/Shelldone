#!/usr/bin/env python3
"""High-level AI agent orchestrator."""
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Dict, List

ROOT = Path(__file__).resolve().parents[2]
if str(ROOT) not in sys.path:
    sys.path.append(str(ROOT))

from scripts.agents.context import generate_context  # type: ignore  # noqa: E402
from scripts import progress  # type: ignore  # noqa: E402
from scripts.agents import heart_engine  # type: ignore  # noqa: E402

TASK_BOARD = ROOT / "data" / "tasks.board.json"
CONFIG_PATH = ROOT / "config" / "agents.json"
DEFAULT_ROLE = "Principal Delivery Engineer"


def load_config() -> Dict[str, object]:
    if CONFIG_PATH.exists():
        with CONFIG_PATH.open("r", encoding="utf-8") as fh:
            return json.load(fh)
    return {
        "default_role": DEFAULT_ROLE,
        "log_dir": "reports/agents",
        "context": {"top_k_chunks": 6, "max_snippet_chars": 320},
        "agents": {},
    }


def ensure_log_dir(path: Path) -> None:
    path.mkdir(parents=True, exist_ok=True)


def build_prompt_file(task_id: str, role: str, agent: str, context_cfg: Dict[str, object]) -> Path:
    top_k = int(context_cfg.get("top_k_chunks", 6))
    prompt_text = generate_context(task_id, role, agent, top_k=top_k)
    temp = Path(tempfile_name(prefix="prompt-", suffix=".md"))
    temp.write_text(prompt_text, encoding="utf-8")
    return temp


def tempfile_name(prefix: str = "tmp-", suffix: str = "") -> str:
    import tempfile

    fd, path = tempfile.mkstemp(prefix=prefix, suffix=suffix)
    os.close(fd)
    return path


def run_subprocess(command: List[str], prompt_path: Path, stdin_mode: bool) -> subprocess.CompletedProcess[str]:
    env = os.environ.copy()
    if stdin_mode:
        prompt_text = prompt_path.read_text(encoding="utf-8")
        return subprocess.run(
            [str(part) for part in command],
            input=prompt_text,
            text=True,
            capture_output=True,
            cwd=ROOT,
            check=False,
        )
    return subprocess.run([str(part) for part in command], text=True, capture_output=True, cwd=ROOT, check=False)


def generate_loopback_response(task_id: str, prompt_path: Path, command: str) -> str:
    state = progress.collect_progress_state()
    chunks = heart_engine.query_chunks(heart_engine.load_config(), task_id or prompt_path.read_text()[:200], top_k=3)
    lines = [
        "{",
        f"  \"mode\": \"loopback\",",
        f"  \"command\": \"{command}\",",
        f"  \"task\": \"{task_id}\",",
        f"  \"program_progress\": {state['program']['progress_pct']},",
        "  \"next_steps\": [",
    ]
    for idx, chunk in enumerate(chunks[:3]):
        snippet = chunk.get("snippet", "").replace("\"", "'")
        lines.append(f"    \"{idx + 1}. Review {chunk['path']} lines {chunk['start_line']}-{chunk['end_line']}: {snippet}\",")
    if not chunks:
        lines.append("    \"1. Review current roadmap and align deliverables\",")
    lines.append("  ]")
    lines.append("}")
    return "\n".join(lines)


def append_comment(task_id: str, author: str, comment: str) -> None:
    if not task_id:
        return
    if not TASK_BOARD.exists():
        return
    data = json.loads(TASK_BOARD.read_text(encoding="utf-8"))
    updated = False
    timestamp = datetime.now(timezone.utc).replace(microsecond=0).isoformat()
    for task in data.get("tasks", []):
        if task.get("id") != task_id:
            continue
        comments = task.setdefault("comments", [])
        comments.append({"author": author, "timestamp": timestamp, "message": comment})
        task.setdefault("owner", author)
        updated = True
        break
    if updated:
        data["updated_at"] = timestamp
        TASK_BOARD.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def summarize_output(stdout: str, max_lines: int = 10) -> str:
    lines = stdout.strip().splitlines()
    return "\n".join(lines[:max_lines])


def command_for_agent(agent_cfg: Dict[str, object]) -> List[str]:
    command = agent_cfg.get("command") or []
    if isinstance(command, list):
        return [str(item) for item in command]
    if isinstance(command, str):
        return [command]
    return []


def add_sandbox(command: List[str], sandbox_mode: str | None) -> List[str]:
    mode = (sandbox_mode or "auto").lower()
    sandbox_exec = ROOT / "scripts" / "bin" / "sandbox_exec"
    if mode == "none":
        return command
    if sandbox_exec.exists():
        return [str(sandbox_exec)] + command
    return command


def main(argv: List[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Run AI agent over project context")
    parser.add_argument("command", choices=["assign", "plan", "analysis"], help="Mode of execution")
    parser.add_argument("--task", default="", help="Task ID (optional for analysis)")
    parser.add_argument("--agent", default="codex", help="Agent key from config")
    parser.add_argument("--role", default="", help="Role prompt override")
    parser.add_argument("--dry-run", action="store_true", help="Only generate context and exit")
    args = parser.parse_args(argv)

    cfg = load_config()
    role = args.role or cfg.get("default_role", DEFAULT_ROLE)
    context_cfg = cfg.get("context", {}) if isinstance(cfg.get("context"), dict) else {}
    prompt_path = Path(tempfile_name(prefix="agent-prompt-", suffix=".md"))
    prompt_path.write_text(generate_context(args.task, role, args.agent, top_k=int(context_cfg.get("top_k_chunks", 6))), encoding="utf-8")

    if args.dry_run:
        print(prompt_path)
        return 0

    agents_cfg = cfg.get("agents", {})
    agent_cfg = agents_cfg.get(args.agent, {}) if isinstance(agents_cfg, dict) else {}
    command_template = command_for_agent(agent_cfg)
    stdin_mode = bool(agent_cfg.get("stdin", False))
    prompt_arg = agent_cfg.get("prompt_arg")

    command: List[str]
    if not command_template:
        # fallback to loopback mode
        stdout = generate_loopback_response(args.task, prompt_path, args.command)
        log_path = Path(cfg.get("log_dir", "reports/agents"))
        ensure_log_dir(log_path)
        log_file = log_path / f"{datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ')}-{args.agent}-{args.command}.log"
        log_file.write_text(stdout, encoding="utf-8")
        print(stdout)
        if args.command == "assign":
            append_comment(args.task, agent_cfg.get("owner", args.agent), summarize_output(stdout, 3))
        return 0

    # Build final command list
    if stdin_mode:
        command = command_template
    else:
        command = command_template.copy()
        if prompt_arg:
            if prompt_arg not in command:
                command.append(prompt_arg)
            command.append(str(prompt_path))
        else:
            command.append(str(prompt_path))
    command = add_sandbox(command, agent_cfg.get("sandbox"))
    log_dir = Path(cfg.get("log_dir", "reports/agents"))
    ensure_log_dir(log_dir)

    try:
        result = run_subprocess(command, prompt_path, stdin_mode=stdin_mode)
    except FileNotFoundError:
        stdout = generate_loopback_response(args.task, prompt_path, args.command)
        log_file = log_dir / f"{datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ')}-{args.agent}-{args.command}.log"
        log_file.write_text(stdout, encoding="utf-8")
        print(stdout)
        if args.command == "assign":
            append_comment(args.task, agent_cfg.get("owner", args.agent), summarize_output(stdout, 3))
        return 0

    timestamp = datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ')
    log_file = log_dir / f"{timestamp}-{args.agent}-{args.command}.log"
    payload = {
        "agent": args.agent,
        "command": args.command,
        "task": args.task,
        "returncode": result.returncode,
        "stdout": result.stdout,
        "stderr": result.stderr,
        "prompt_file": str(prompt_path),
    }
    log_file.write_text(json.dumps(payload, ensure_ascii=False, indent=2), encoding="utf-8")

    if result.returncode != 0 or not result.stdout.strip():
        stdout = generate_loopback_response(args.task, prompt_path, args.command)
        print(stdout)
        if args.command == "assign":
            append_comment(args.task, agent_cfg.get("owner", args.agent), summarize_output(stdout, 3))
        return 0

    print(result.stdout.strip())
    if args.command == "assign":
        append_comment(args.task, agent_cfg.get("owner", args.agent), summarize_output(result.stdout, 3))
    return result.returncode


if __name__ == "__main__":
    raise SystemExit(main())

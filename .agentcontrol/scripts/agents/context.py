#!/usr/bin/env python3
"""Generate rich context prompts for AI agents."""
from __future__ import annotations

import argparse
import json
import subprocess
import sys
import tempfile
import textwrap
from datetime import datetime, timezone
from pathlib import Path
from typing import List

ROOT = Path(__file__).resolve().parents[2]
if str(ROOT) not in sys.path:
    sys.path.append(str(ROOT))

from scripts import progress  # type: ignore  # noqa: E402
from scripts.agents import heart_engine  # type: ignore  # noqa: E402

TASK_BOARD = ROOT / "data" / "tasks.board.json"


def load_task(task_id: str) -> dict | None:
    if not task_id:
        return None
    if not TASK_BOARD.exists():
        return None
    data = json.loads(TASK_BOARD.read_text(encoding="utf-8"))
    for task in data.get("tasks", []):
        if task.get("id") == task_id:
            return task
    return None


def git_capture(cmd: List[str]) -> str:
    result = subprocess.run(cmd, cwd=ROOT, text=True, capture_output=True, check=False)
    return result.stdout.strip()


def generate_context(task_id: str, role: str | None, agent: str | None, top_k: int = 6) -> str:
    manifest = progress.load_manifest()
    program_progress, epic_progress, big_progress, phase_progress = progress.calculate_progress(manifest)
    tables = progress.render_progress_tables(program_progress, epic_progress, big_progress, manifest)

    task = load_task(task_id)

    query_text = task.get("title") if task else ""
    if task and task.get("success_criteria"):
        query_text += "\n" + "\n".join(task.get("success_criteria", []))
    if not query_text:
        query_text = "overall project status"

    cfg = heart_engine.load_config()
    chunks = heart_engine.query_chunks(cfg, query_text, top_k=top_k)

    timestamp = datetime.now(timezone.utc).replace(microsecond=0).isoformat()
    lines = [f"# AI Agent Context ({timestamp})"]
    if role:
        lines.append(f"**Assigned Role:** {role}")
    if agent:
        lines.append(f"**Agent:** {agent}")
    lines.append("")

    if task:
        lines.append("## Task Overview")
        lines.append(f"- **ID:** {task.get('id')}")
        lines.append(f"- **Title:** {task.get('title')}")
        lines.append(f"- **Status:** {task.get('status')}")
        lines.append(f"- **Priority:** {task.get('priority')}")
        lines.append(f"- **Owner:** {task.get('owner', 'unassigned')}")
        lines.append(f"- **Big Task:** {task.get('big_task')}")
        lines.append(f"- **System:** {task.get('system')}")
        if task.get("success_criteria"):
            lines.append("- **Success Criteria:**")
            for item in task.get("success_criteria", []):
                lines.append(f"  - {item}")
        if task.get("failure_criteria"):
            lines.append("- **Failure Criteria:**")
            for item in task.get("failure_criteria", []):
                lines.append(f"  - {item}")
        if task.get("blockers"):
            lines.append("- **Blockers:**")
            for item in task.get("blockers", []):
                lines.append(f"  - {item}")
        lines.append("")

    lines.append("## Progress Snapshot")
    lines.append("```\n" + tables + "\n```")

    lines.append("## Roadmap Phase Progress")
    phase_lines = [f"- {phase}: {value}%" for phase, value in phase_progress.items()]
    lines.extend(phase_lines)
    lines.append("")

    if chunks:
        lines.append("## Relevant Knowledge Chunks")
        for idx, chunk in enumerate(chunks, start=1):
            snippet = textwrap.indent(chunk.get("snippet", ""), "> ")
            lines.append(
                textwrap.dedent(
                    f"""{idx}. **{chunk['path']}:{chunk['start_line']}-{chunk['end_line']}** (score {chunk['score']})\n{snippet}"""
                )
            )
        lines.append("")

    status = git_capture(["git", "status", "--short"])
    if status:
        lines.append("## Git Status")
        lines.append("```\n" + status + "\n```")
    diffstat = git_capture(["git", "diff", "--stat", "HEAD"])
    if diffstat:
        lines.append("## Git Diff (stat)")
        lines.append("```\n" + diffstat + "\n```")

    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Generate context for AI agent")
    parser.add_argument("--task", default="", help="Task ID to focus on")
    parser.add_argument("--role", default="", help="Role instructions")
    parser.add_argument("--agent", default="", help="Agent name")
    parser.add_argument("--top-k", type=int, default=6, help="Heart chunks to include")
    parser.add_argument("--output", default="", help="Output file path")
    args = parser.parse_args(argv)

    context = generate_context(args.task, args.role or None, args.agent or None, top_k=args.top_k)

    if args.output:
        Path(args.output).write_text(context, encoding="utf-8")
        print(args.output)
    else:
        temp = tempfile.NamedTemporaryFile(delete=False, suffix=".md", prefix="agent-context-")
        temp.write(context.encode("utf-8"))
        temp.flush()
        temp.close()
        print(temp.name)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

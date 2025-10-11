#!/usr/bin/env python3
"""Run agent workflows (assign, review, etc.)."""
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, Optional

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_CONFIG_PATH = ROOT / "config" / "agents.json"
RUNNER_PATH = ROOT / "scripts" / "agents" / "run.py"


@dataclass
class Workflow:
    name: str
    assign_agent: str
    assign_role: Optional[str]
    review_agent: Optional[str]
    review_role: Optional[str]


def resolve_config_path() -> Path:
    override = os.environ.get("AGENTS_CONFIG_PATH")
    if override:
        return Path(override).expanduser()
    return DEFAULT_CONFIG_PATH


def load_config(path: Path) -> Dict[str, object]:
    with path.open("r", encoding="utf-8") as fh:
        return json.load(fh)


def pick_workflow(cfg: Dict[str, object], name: str | None) -> Workflow:
    workflows = cfg.get("workflows", {}) if isinstance(cfg.get("workflows"), dict) else {}
    if name is None:
        name = "default"
    wf_cfg = workflows.get(name, {}) if isinstance(workflows, dict) else {}
    assign_agent = os.environ.get("ASSIGN_AGENT") or wf_cfg.get("assign_agent") or cfg.get("default_agent") or "codex"
    assign_role = os.environ.get("ASSIGN_ROLE") or wf_cfg.get("assign_role")
    review_agent = os.environ.get("REVIEW_AGENT") or wf_cfg.get("review_agent") or None
    review_role = os.environ.get("REVIEW_ROLE") or wf_cfg.get("review_role") or None
    return Workflow(
        name=name,
        assign_agent=str(assign_agent),
        assign_role=str(assign_role) if assign_role else None,
        review_agent=str(review_agent) if review_agent else None,
        review_role=str(review_role) if review_role else None,
    )


def run_agent(command: str, agent: str, task: str, role: Optional[str]) -> subprocess.CompletedProcess[str]:
    args = [sys.executable, str(RUNNER_PATH), command, "--agent", agent]
    if task:
        args.extend(["--task", task])
    if role:
        args.extend(["--role", role])
    result = subprocess.run(args, text=True, capture_output=True)
    return result


def print_step(title: str) -> None:
    print(f"\n== {title} ==")


def pipeline(task_id: str, workflow: Workflow, dry_run: bool) -> int:
    print_step(f"Assign via {workflow.assign_agent}")
    assign_result = run_agent("assign", workflow.assign_agent, task_id, workflow.assign_role)
    if assign_result.returncode != 0:
        sys.stderr.write(assign_result.stderr)
        print(assign_result.stdout.strip())
        return assign_result.returncode
    print(assign_result.stdout.strip())
    if dry_run:
        return 0
    if workflow.review_agent:
        print_step(f"Review via {workflow.review_agent}")
        review_result = run_agent("analysis", workflow.review_agent, task_id, workflow.review_role)
        if review_result.returncode != 0:
            sys.stderr.write(review_result.stderr)
            print(review_result.stdout.strip())
            return review_result.returncode
        print(review_result.stdout.strip())
    return 0


def assign_only(task_id: str, workflow: Workflow) -> int:
    print_step(f"Assign via {workflow.assign_agent}")
    result = run_agent("assign", workflow.assign_agent, task_id, workflow.assign_role)
    if result.returncode != 0:
        sys.stderr.write(result.stderr)
    print(result.stdout.strip())
    return result.returncode


def review_only(task_id: str, workflow: Workflow) -> int:
    if not workflow.review_agent:
        print("Review agent не задан в workflow", file=sys.stderr)
        return 1
    print_step(f"Review via {workflow.review_agent}")
    result = run_agent("analysis", workflow.review_agent, task_id, workflow.review_role)
    if result.returncode != 0:
        sys.stderr.write(result.stderr)
    print(result.stdout.strip())
    return result.returncode


def main() -> int:
    parser = argparse.ArgumentParser(description="Run agent workflows")
    sub = parser.add_subparsers(dest="mode", required=True)

    shared = argparse.ArgumentParser(add_help=False)
    shared.add_argument("--task", required=False, default="", help="Task identifier")
    shared.add_argument("--workflow", required=False, help="Workflow name (default=default)")

    sub.add_parser("assign", parents=[shared], help="Run only the assign step")
    sub.add_parser("review", parents=[shared], help="Run only the review step")
    pipeline_parser = sub.add_parser("pipeline", parents=[shared], help="Assign then review")
    pipeline_parser.add_argument("--dry-run", action="store_true", help="Skip review step")

    args = parser.parse_args()
    cfg = load_config(resolve_config_path())
    task_id = args.task or os.environ.get("TASK", "")
    wf_name = args.workflow or os.environ.get("WORKFLOW")
    wf = pick_workflow(cfg, wf_name)

    if args.mode == "assign":
        return assign_only(task_id, wf)
    if args.mode == "review":
        return review_only(task_id, wf)
    if args.mode == "pipeline":
        dry_run = args.dry_run or os.environ.get("DRY_RUN") == "1"
        return pipeline(task_id, wf, dry_run)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())

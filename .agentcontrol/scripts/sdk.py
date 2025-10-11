#!/usr/bin/env python3
"""Единый CLI для управления GPT-5 Codex SDK."""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPTS = ROOT / "scripts"


def run(command: list[str]) -> int:
    proc = subprocess.run(command)
    return proc.returncode


def cmd_verify(args: argparse.Namespace) -> int:
    return run([str(SCRIPTS / "verify.sh")])


def cmd_review(args: argparse.Namespace) -> int:
    script = str(SCRIPTS / "review.sh")
    if args.base:
        return run(["env", f"REVIEW_BASE_REF={args.base}", script])
    return run([script])


def cmd_doctor(args: argparse.Namespace) -> int:
    return run([str(SCRIPTS / "doctor.sh")])


def cmd_status(args: argparse.Namespace) -> int:
    return run([str(SCRIPTS / "status.sh")])


def cmd_summary(args: argparse.Namespace) -> int:
    script = ["python3", str(SCRIPTS / "lib" / "report_summary.py")]
    return run(script)


def cmd_task(args: argparse.Namespace) -> int:
    return run([str(SCRIPTS / "task.sh"), *args.args])


def cmd_call(args: argparse.Namespace) -> int:
    return run(["agentcall", "run", args.target, *args.args])


def cmd_qa(args: argparse.Namespace) -> int:
    if cmd_verify(args) != 0:
        return 1
    return cmd_review(args)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="sdk", description="GPT-5 Codex SDK helper")
    sub = parser.add_subparsers(dest="command", required=True)

    verify = sub.add_parser("verify", help="Запустить agentcall verify")
    verify.set_defaults(func=cmd_verify)

    review = sub.add_parser("review", help="Запустить agentcall review")
    review.add_argument("--base", help="Базовый коммит для diff", default=None)
    review.set_defaults(func=cmd_review)

    doctor = sub.add_parser("doctor", help="Проверка окружения и зависимостей")
    doctor.set_defaults(func=cmd_doctor)

    status = sub.add_parser("status", help="agentcall status")
    status.set_defaults(func=cmd_status)

    summary = sub.add_parser("summary", help="Сводка verify/review/doctor")
    summary.set_defaults(func=cmd_summary)

    task = sub.add_parser("task", help="Прокси к scripts/task.sh")
    task.add_argument("args", nargs=argparse.REMAINDER)
    task.set_defaults(func=cmd_task)

    call = sub.add_parser("run", help="Проброс команды в agentcall run <name>")
    call.add_argument("target")
    call.add_argument("args", nargs=argparse.REMAINDER)
    call.set_defaults(func=cmd_call)

    qa = sub.add_parser("qa", help="verify -> review")
    qa.add_argument("--base", help="Базовый коммит для review", default=None)
    qa.set_defaults(func=cmd_qa)

    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())

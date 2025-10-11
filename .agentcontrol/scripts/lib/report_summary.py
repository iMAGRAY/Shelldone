#!/usr/bin/env python3
"""Сводный обзор отчетов verify/review/doctor."""

from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Dict, Optional


@dataclass
class ReportInfo:
    path: Path
    exists: bool
    data: Optional[Dict[str, Any]]


def load(path: Path) -> ReportInfo:
    if not path.exists():
        return ReportInfo(path, False, None)
    try:
        return ReportInfo(path, True, json.loads(path.read_text(encoding="utf-8")))
    except json.JSONDecodeError:
        return ReportInfo(path, True, None)


def summarize_verify(data: Optional[Dict[str, Any]]) -> str:
    if not data:
        return "verify: отчёт недоступен"
    exit_code = data.get("exit_code")
    steps = data.get("steps", [])
    failed = [s for s in steps if s.get("status") == "fail"]
    return "verify: OK" if exit_code == 0 and not failed else f"verify: FAIL (exit={exit_code}, failed={len(failed)})"


def summarize_review(data: Optional[Dict[str, Any]]) -> str:
    if not data:
        return "review: отчёт недоступен"
    exit_code = data.get("exit_code")
    return "review: OK" if exit_code == 0 else f"review: FAIL (exit={exit_code})"


def summarize_doctor(data: Optional[Dict[str, Any]]) -> str:
    if not data:
        return "doctor: отчёт недоступен"
    problems = [r for r in data.get("results", []) if r.get("status") == "missing"]
    return "doctor: OK" if not problems else f"doctor: требуется {len(problems)} исправлений"


def build_summary(root: Path) -> Dict[str, str]:
    verify = load(root / "reports" / "verify.json")
    review = load(root / "reports" / "review.json")
    doctor = load(root / "reports" / "doctor.json")
    return {
        "verify": summarize_verify(verify.data if verify.exists else None),
        "review": summarize_review(review.data if review.exists else None),
        "doctor": summarize_doctor(doctor.data if doctor.exists else None),
    }


def main() -> int:
    root = Path.cwd()
    summary = build_summary(root)
    for key, value in summary.items():
        print(value)
    return 0


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())

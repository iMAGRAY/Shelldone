from __future__ import annotations

import json
import time
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import List, Sequence

from ..domain.probe import ProbeAggregate, ProbeReport, ProbeSpec
from ..ports.runner import ProbeRunnerPort


@dataclass(frozen=True)
class SuiteReport:
    reports: List[ProbeReport]
    artifact_paths: List[str]
    generated_at: str

    def has_failures(self) -> bool:
        return any(report.has_failures() for report in self.reports)

    def violated_budgets(self) -> List[str]:
        violations: List[str] = []
        for report in self.reports:
            for budget in report.violated_budgets():
                violations.append(
                    f"{report.probe_id}:{budget.alias} actual={budget.actual:.2f}{budget.unit}"
                )
        return violations

    def to_dict(self) -> dict:
        return {
            "generated_at": self.generated_at,
            "artifacts": list(self.artifact_paths),
            "probes": [
                {
                    "probe_id": report.probe_id,
                    "label": report.label,
                    "aggregated": {
                        alias: {"value": metric.value, "unit": metric.unit}
                        for alias, metric in report.aggregated.items()
                    },
                    "budgets": [
                        {
                            "alias": budget.alias,
                            "comparator": budget.comparator,
                            "limit": budget.limit,
                            "unit": budget.unit,
                            "actual": budget.actual,
                            "passed": budget.passed,
                        }
                        for budget in report.budgets
                    ],
                    "violations": [
                        {
                            "alias": budget.alias,
                            "actual": budget.actual,
                            "limit": budget.limit,
                            "unit": budget.unit,
                        }
                        for budget in report.budgets
                        if not budget.passed
                    ],
                }
                for report in self.reports
            ],
        }


class PerfProbeService:
    def __init__(self, runner: ProbeRunnerPort, artifacts_root: Path) -> None:
        self.runner = runner
        self.artifacts_root = artifacts_root

    def run_suite(self, specs: Sequence[ProbeSpec]) -> SuiteReport:
        self.artifacts_root.mkdir(parents=True, exist_ok=True)
        reports: List[ProbeReport] = []
        written_artifacts: List[str] = []

        for spec in specs:
            probe_dir = self.artifacts_root / spec.probe_id
            probe_dir.mkdir(parents=True, exist_ok=True)
            aggregate = ProbeAggregate(spec)
            if spec.warmup_seconds > 0:
                time.sleep(spec.warmup_seconds)
            for trial_index in range(spec.trials):
                result = self.runner.run(spec, trial_index, probe_dir)
                aggregate.add_trial(result)
                written_artifacts.append(result.summary_path)
            report = aggregate.to_report()
            reports.append(report)
            summary_path = probe_dir / "summary.json"
            self._write_report(summary_path, report)
            written_artifacts.append(str(summary_path))

        suite_path = self.artifacts_root / "summary.json"
        written_artifacts.append(str(suite_path))
        generated_at = datetime.utcnow().isoformat()
        suite_report = SuiteReport(
            reports=reports,
            artifact_paths=written_artifacts.copy(),
            generated_at=generated_at,
        )
        self._write_suite_summary(suite_path, suite_report)

        return suite_report

    def _write_report(self, path: Path, report: ProbeReport) -> None:
        path.write_text(json.dumps(report.to_dict(), indent=2, sort_keys=True), encoding="utf-8")

    def _write_suite_summary(self, path: Path, suite_report: SuiteReport) -> None:
        path.write_text(json.dumps(suite_report.to_dict(), indent=2, sort_keys=True), encoding="utf-8")

    def export_suite_summary(self, path: Path, suite_report: SuiteReport) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(suite_report.to_dict(), indent=2, sort_keys=True), encoding="utf-8")

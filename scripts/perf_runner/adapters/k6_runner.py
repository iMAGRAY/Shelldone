from __future__ import annotations

import json
import os
import shutil
import subprocess
from datetime import datetime
from pathlib import Path
from typing import Dict

from ..domain.probe import ProbeSpec, ProbeTrialResult
from ..domain.value_objects import MetricDefinition, MetricValue
from ..ports.runner import ProbeExecutionError, ProbeRunnerPort


class K6Runner(ProbeRunnerPort):
    """Executes k6 scripts and extracts configured metrics."""

    def __init__(self, binary: str = "k6") -> None:
        self.binary = binary

    def run(
        self,
        spec: ProbeSpec,
        trial_index: int,
        output_dir: Path,
    ) -> ProbeTrialResult:
        summary_path = self._summary_path(spec, output_dir, trial_index)
        output_dir.mkdir(parents=True, exist_ok=True)

        if shutil.which(self.binary) is None:
            raise ProbeExecutionError(f"{self.binary} is required but not available in PATH")

        env = os.environ.copy()
        env.update(
            {
                "SHELLDONE_PERF_WARMUP_SEC": str(spec.warmup_seconds),
                "SHELLDONE_PERF_TRIAL_INDEX": str(trial_index + 1),
                "SHELLDONE_PERF_TOTAL_TRIALS": str(spec.trials),
                "SHELLDONE_PERF_STARTED_AT": datetime.utcnow().isoformat(),
            }
        )
        env.update(spec.extra_env)

        command = [
            self.binary,
            "run",
            "--quiet",
            f"--summary-export={summary_path}",
            str(spec.script.path),
        ]

        try:
            subprocess.run(command, check=True, env=env, cwd=str(spec.script.path.parent))
        except subprocess.CalledProcessError as exc:  # pragma: no cover (propagated)
            raise ProbeExecutionError(
                f"k6 execution failed for probe {spec.probe_id}: {exc}"
            ) from exc

        metrics = self._extract_metrics(spec, summary_path)
        return ProbeTrialResult(
            trial_index=trial_index,
            metrics=metrics,
            summary_path=str(summary_path),
        )

    @staticmethod
    def _extract_metrics(spec: ProbeSpec, summary_path: Path) -> Dict[str, MetricValue]:
        try:
            with summary_path.open("r", encoding="utf-8") as handle:
                payload = json.load(handle)
        except OSError as exc:  # pragma: no cover (pass-through)
            raise ProbeExecutionError(
                f"unable to read k6 summary for probe {spec.probe_id}: {exc}"
            ) from exc
        except json.JSONDecodeError as exc:  # pragma: no cover
            raise ProbeExecutionError(
                f"invalid JSON summary for probe {spec.probe_id}: {exc}"
            ) from exc

        metrics_section = payload.get("metrics", {})
        result: Dict[str, MetricValue] = {}
        for definition in spec.metrics:
            metric_payload = metrics_section.get(definition.metric_id)
            if not isinstance(metric_payload, dict):
                raise ProbeExecutionError(
                    f"metric '{definition.metric_id}' missing in k6 summary"
                )
            values = metric_payload.get("values")
            if not isinstance(values, dict):
                raise ProbeExecutionError(
                    f"metric '{definition.metric_id}' has no 'values' section"
                )
            if definition.statistic not in values:
                raise ProbeExecutionError(
                    f"metric '{definition.metric_id}' missing statistic {definition.statistic}"
                )
            raw_value = values[definition.statistic]
            try:
                value = float(raw_value)
            except (TypeError, ValueError) as exc:
                raise ProbeExecutionError(
                    f"metric '{definition.metric_id}' statistic {definition.statistic} is not numeric"
                ) from exc
            result[definition.alias] = MetricValue(
                alias=definition.alias,
                value=value,
                unit=definition.unit,
            )
        return result

    @staticmethod
    def _summary_path(spec: ProbeSpec, output_dir: Path, trial_index: int) -> Path:
        suffix = f"trial{trial_index + 1}.json"
        return output_dir / f"{spec.summary_prefix}_{suffix}"

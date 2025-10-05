from __future__ import annotations

import json
import os
import platform
import shutil
import subprocess
import tarfile
import tempfile
import urllib.request
from datetime import datetime
from pathlib import Path
from typing import Dict, Optional

from ..domain.probe import ProbeSpec, ProbeTrialResult
from ..domain.value_objects import MetricDefinition, MetricValue
from ..ports.runner import ProbeExecutionError, ProbeRunnerPort


class K6Runner(ProbeRunnerPort):
    """Executes k6 scripts and extracts configured metrics."""

    DEFAULT_VERSION = "v0.48.0"

    def __init__(
        self,
        binary: str = "k6",
        *,
        auto_install: bool = True,
        version: str = DEFAULT_VERSION,
    ) -> None:
        self.binary = binary
        self.auto_install = auto_install
        self.version = version
        self.resolved_binary: Optional[str] = None

    def run(
        self,
        spec: ProbeSpec,
        trial_index: int,
        output_dir: Path,
    ) -> ProbeTrialResult:
        output_dir = output_dir.resolve()
        summary_path = self._summary_path(spec, output_dir, trial_index)
        summary_path.parent.mkdir(parents=True, exist_ok=True)

        binary = self._ensure_binary()

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
            binary,
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
            if isinstance(values, dict):
                lookup = values
            else:
                lookup = metric_payload
            if definition.statistic not in lookup:
                raise ProbeExecutionError(
                    f"metric '{definition.metric_id}' missing statistic {definition.statistic}"
                )
            raw_value = lookup[definition.statistic]
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

    def _ensure_binary(self) -> str:
        if self.resolved_binary:
            return self.resolved_binary

        found = shutil.which(self.binary)
        if found:
            self.resolved_binary = found
            return found

        if not self.auto_install:
            raise ProbeExecutionError(f"{self.binary} is required but not available in PATH")

        cache_dir = Path.home() / ".cache" / "shelldone" / "k6" / self.version
        target = cache_dir / "k6"
        if target.exists():
            target.chmod(0o755)
            self.resolved_binary = str(target)
            return self.resolved_binary

        cache_dir.mkdir(parents=True, exist_ok=True)

        system = platform.system().lower()
        arch = platform.machine().lower()
        if system.startswith("linux") and arch in {"x86_64", "amd64"}:
            archive_name = f"k6-{self.version}-linux-amd64.tar.gz"
            inner_path = f"k6-{self.version}-linux-amd64/k6"
        elif system.startswith("darwin") and arch in {"x86_64", "amd64"}:
            archive_name = f"k6-{self.version}-macos-amd64.tar.gz"
            inner_path = f"k6-{self.version}-macos-amd64/k6"
        else:
            raise ProbeExecutionError(
                f"auto-installation for k6 is not supported on platform {platform.system()} ({platform.machine()})"
            )

        url = (
            f"https://github.com/grafana/k6/releases/download/{self.version}/{archive_name}"
        )

        with tempfile.TemporaryDirectory(prefix="k6-download-") as tmp_dir:
            archive_path = Path(tmp_dir) / archive_name
            try:
                with urllib.request.urlopen(url, timeout=30) as response, archive_path.open(
                    "wb"
                ) as handle:
                    shutil.copyfileobj(response, handle)
            except Exception as exc:  # pragma: no cover
                raise ProbeExecutionError(f"failed to download k6 binary: {exc}") from exc

            try:
                with tarfile.open(archive_path, "r:gz") as tar:
                    member = tar.getmember(inner_path)
                    tar.extract(member, path=tmp_dir)
                    extracted = Path(tmp_dir) / inner_path
                    shutil.copy2(extracted, target)
                    target.chmod(0o755)
            except (KeyError, tarfile.TarError, OSError) as exc:
                raise ProbeExecutionError(f"failed to extract k6 binary: {exc}") from exc

        self.resolved_binary = str(target)
        return self.resolved_binary

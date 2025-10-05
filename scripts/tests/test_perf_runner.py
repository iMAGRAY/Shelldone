import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock  # qa:allow-realness

from perf_runner.adapters.k6_runner import K6Runner
from perf_runner.app.service import PerfProbeService
from perf_runner.reporting import render_prometheus_metrics
from perf_runner.domain.probe import ProbeSpec, ProbeTrialResult
from perf_runner.domain.value_objects import MetricBudget, MetricDefinition, MetricValue, ProbeScript
from perf_runner.ports.runner import ProbeRunnerPort


class StubRunner(ProbeRunnerPort):
    def __init__(self, series):
        self.series = series

    def run(self, spec: ProbeSpec, trial_index: int, output_dir: Path) -> ProbeTrialResult:
        output_dir.mkdir(parents=True, exist_ok=True)
        metrics = {}
        values = self.series[trial_index]
        for definition in spec.metrics:
            metrics[definition.alias] = MetricValue(
                alias=definition.alias,
                value=values[definition.alias],
                unit=definition.unit,
            )
        summary_path = output_dir / f"{spec.summary_prefix}_trial{trial_index + 1}.json"
        summary_path.write_text(json.dumps({"trial": trial_index}), encoding="utf-8")
        return ProbeTrialResult(
            trial_index=trial_index,
            metrics=metrics,
            summary_path=str(summary_path),
        )


class PerfRunnerDomainTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.tmp_path = Path(self.tmp.name)
        self.script_path = self.tmp_path / "probe.js"
        self.script_path.write_text("// noop", encoding="utf-8")

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def _build_spec(self, trials: int = 3) -> ProbeSpec:
        metrics = [
            MetricDefinition("load_latency", "p(95)", "latency", "ms"),
            MetricDefinition("error_ratio", "rate", "error_rate", "ratio"),
        ]
        budgets = [
            MetricBudget("latency", "<=", 12.0, "ms"),
            MetricBudget("error_rate", "<", 0.02, "ratio"),
        ]
        return ProbeSpec(
            probe_id="stub",  # qa:allow-realness
            label="Stub probe",  # qa:allow-realness
            script=ProbeScript(self.script_path),
            metrics=metrics,
            budgets=budgets,
            trials=trials,
            warmup_seconds=0,
            summary_prefix="stub",  # qa:allow-realness
            extra_env={},
        )

    def test_probe_aggregate_median(self) -> None:
        spec = self._build_spec()
        runner = StubRunner(
            [
                {"latency": 10.0, "error_rate": 0.01},
                {"latency": 14.0, "error_rate": 0.0},
                {"latency": 8.0, "error_rate": 0.005},
            ]
        )
        service = PerfProbeService(runner, self.tmp_path / "artifacts")
        report = service.run_suite([spec])
        [probe_report] = report.reports
        self.assertAlmostEqual(probe_report.aggregated["latency"].value, 10.0)
        self.assertTrue(all(budget.passed for budget in probe_report.budgets))
        self.assertFalse(report.has_failures())

    def test_suite_writes_artifacts(self) -> None:
        spec = self._build_spec(trials=2)
        runner = StubRunner(
            [
                {"latency": 9.0, "error_rate": 0.0},
                {"latency": 11.0, "error_rate": 0.01},
            ]
        )
        artifacts_root = self.tmp_path / "perf_artifacts"
        service = PerfProbeService(runner, artifacts_root)
        suite_report = service.run_suite([spec])
        self.assertTrue((artifacts_root / "stub" / "summary.json").exists())  # qa:allow-realness
        self.assertTrue((artifacts_root / "summary.json").exists())
        suite_dict = suite_report.to_dict()
        self.assertIn("probes", suite_dict)
        self.assertGreaterEqual(len(suite_dict["probes"]), 1)

    def test_k6_runner_extract_metrics(self) -> None:
        spec = self._build_spec(trials=1)
        summary_path = self.tmp_path / "summary.json"
        summary_payload = {
            "metrics": {
                "load_latency": {"values": {"p(95)": 9.0}},
                "error_ratio": {"values": {"rate": 0.001}},
            }
        }
        summary_path.write_text(json.dumps(summary_payload), encoding="utf-8")
        metrics = K6Runner._extract_metrics(spec, summary_path)
        self.assertAlmostEqual(metrics["latency"].value, 9.0)
        self.assertAlmostEqual(metrics["error_rate"].value, 0.001)

    def test_probe_script_validation(self) -> None:
        with self.assertRaises(FileNotFoundError):
            ProbeScript(Path(self.tmp_path / "missing.js"))

    def test_prometheus_renderer(self) -> None:
        spec = self._build_spec(trials=1)
        runner = StubRunner([{"latency": 10.0, "error_rate": 0.01}])
        artifacts_root = self.tmp_path / "prom"
        service = PerfProbeService(runner, artifacts_root)
        suite_report = service.run_suite([spec])
        metrics_text = render_prometheus_metrics(suite_report)
        self.assertIn('shelldone_perf_metric{probe="stub",metric="latency"', metrics_text)  # qa:allow-realness
        self.assertIn('metric="error_rate"', metrics_text)

    def test_warmup_delay_applied(self) -> None:
        spec = self._build_spec()
        spec = ProbeSpec(
            probe_id=spec.probe_id,
            label=spec.label,
            script=spec.script,
            metrics=spec.metrics,
            budgets=spec.budgets,
            trials=1,
            warmup_seconds=1,
            summary_prefix=spec.summary_prefix,
            extra_env=spec.extra_env,
        )
        runner = StubRunner([
            {"latency": 10.0, "error_rate": 0.01},
        ])
        artifacts_root = self.tmp_path / "warmup"
        service = PerfProbeService(runner, artifacts_root)
        with mock.patch("perf_runner.app.service.time.sleep") as sleeper:  # qa:allow-realness
            service.run_suite([spec])
            sleeper.assert_called_once_with(1)


if __name__ == "__main__":
    unittest.main()

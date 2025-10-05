import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock  # qa:allow-realness

import perf_runner.__main__ as perf_cli
from perf_runner.__main__ import run_cli


class PerfCliTests(unittest.TestCase):
    def test_stub_runner_generates_artifacts(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            artifacts = Path(tmp) / "artifacts"
            exit_code = run_cli(
                [
                    "run",
                    "--runner",
                    "stub",  # qa:allow-realness
                    "--artifacts-dir",
                    str(artifacts),
                    "--no-agentd",
                    "--trials",
                    "2",
                    "--warmup-sec",
                    "0",
                ]
            )
            self.assertEqual(exit_code, 0)
            summary = artifacts / "summary.json"
            self.assertTrue(summary.exists(), "summary.json missing")
            data = json.loads(summary.read_text(encoding="utf-8"))
            self.assertIn("probes", data)
            self.assertGreaterEqual(len(data["probes"]), 2)

    def test_profile_and_summary_export(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            artifacts = Path(tmp) / "artifacts"
            summary_path = Path(tmp) / "export.json"
            exit_code = run_cli(
                [
                    "run",
                    "--runner",
                    "stub",  # qa:allow-realness
                    "--profile",
                    "dev",
                    "--artifacts-dir",
                    str(artifacts),
                    "--no-agentd",
                    "--summary-path",
                    str(summary_path),
                ]
            )
            self.assertEqual(exit_code, 0)
            self.assertTrue(summary_path.exists(), "custom summary not written")
            data = json.loads(summary_path.read_text(encoding="utf-8"))
            self.assertIn("probes", data)
            self.assertGreaterEqual(len(data["probes"]), 1)
            prom_default = Path("reports/perf/metrics.prom")
            self.assertTrue(prom_default.exists(), "default prom metrics missing")

    @mock.patch("perf_runner.__main__.start_agentd")  # qa:allow-realness
    @mock.patch("perf_runner.__main__.wait_for_agentd")  # qa:allow-realness
    @mock.patch("perf_runner.__main__.stop_agentd")  # qa:allow-realness
    def test_k6_runner_allows_no_agentd_flag(self, stop_agentd, wait_for_agentd, start_agentd):
        with tempfile.TemporaryDirectory() as tmp:
            artifacts = Path(tmp) / "artifacts"
            with mock.patch.object(perf_cli, "K6Runner", return_value=perf_cli.StubRunner()):  # qa:allow-realness
                exit_code = run_cli(
                    [
                        "run",
                        "--runner",
                        "k6",
                        "--no-agentd",
                        "--artifacts-dir",
                        str(artifacts),
                        "--trials",
                        "1",
                    ]
                )
            self.assertEqual(exit_code, 0)
            start_agentd.assert_not_called()
            wait_for_agentd.assert_not_called()
            stop_agentd.assert_not_called()


if __name__ == "__main__":
    unittest.main()

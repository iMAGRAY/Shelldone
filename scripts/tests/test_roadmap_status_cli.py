import json
import os
import subprocess
import unittest
from pathlib import Path

import yaml


class RoadmapStatusCliTests(unittest.TestCase):
    ROOT = Path(__file__).resolve().parents[2]
    SCRIPT = ROOT / "agentcontrol" / "scripts" / "roadmap-status.sh"
    MANIFEST = ROOT / "agentcontrol" / "architecture" / "manifest.yaml"

    def run_cli(self) -> dict:
        env = os.environ.copy()
        env["ROADMAP_SKIP_PROGRESS"] = "1"
        completed = subprocess.run(
            [str(self.SCRIPT), "json"],
            cwd=self.ROOT,
            check=True,
            capture_output=True,
            text=True,
            env=env,
        )
        payload = completed.stdout.strip()
        self.assertTrue(payload, "roadmap-status produced empty output")
        return json.loads(payload)

    def test_json_output_matches_manifest(self):
        data = self.run_cli()
        manifest = yaml.safe_load(self.MANIFEST.read_text(encoding="utf-8"))
        expected_epics = {epic["id"]: epic for epic in manifest.get("epics", [])}
        expected_big = {bt["id"]: bt for bt in manifest.get("big_tasks", [])}

        self.assertEqual(data["program"]["progress_pct"], data["program"]["computed_progress_pct"])
        self.assertFalse(data.get("warnings"), f"Unexpected warnings: {data['warnings']}")

        self.assertEqual(len(data["epics"]), len(expected_epics))
        for epic in data["epics"]:
            ref = expected_epics.get(epic["id"])
            self.assertIsNotNone(ref, f"Epic {epic['id']} missing in manifest")
            self.assertAlmostEqual(epic["computed_progress_pct"], ref.get("metrics", {}).get("progress_pct", ref.get("progress_pct", 0)), delta=1)

        self.assertEqual(len(data["big_tasks"]), len(expected_big))
        for bt in data["big_tasks"]:
            ref = expected_big.get(bt["id"])
            self.assertIsNotNone(ref, f"Big task {bt['id']} missing in manifest")
            self.assertAlmostEqual(bt["computed_progress_pct"], ref.get("metrics", {}).get("progress_pct", ref.get("progress_pct", 0)), delta=1)


if __name__ == "__main__":
    unittest.main()

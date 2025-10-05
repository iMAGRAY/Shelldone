#!/usr/bin/env python3
"""Shelldone QA orchestrator."""
from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import textwrap
import time
import shutil
import tempfile
from collections import Counter, defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import Callable, Dict, Iterable, List, Optional, Sequence, Tuple

import yaml

from perf_runner.app.service import PerfProbeService
from perf_runner.adapters.k6_runner import K6Runner
from perf_runner.domain.probe import ProbeSpec
from perf_runner.domain.value_objects import (
    MetricBudget,
    MetricDefinition,
    MetricValue,
    ProbeScript,
)
from perf_runner.infra.agentd import start_agentd, stop_agentd, wait_for_agentd
from perf_runner.profiles import apply_env_overrides, get_profile
from perf_runner.reporting import render_prometheus_metrics
from perf_runner.specs import default_specs

ROOT = Path(__file__).resolve().parent.parent
ARTIFACTS_DIR = ROOT / "artifacts" / "verify"
BASELINE_DIR = ROOT / "qa" / "baselines"
MARKER_BASELINE = BASELINE_DIR / "banned_markers.json"

PASS = "PASS"
FAIL = "FAIL"
SKIP = "SKIP"


class CheckFailure(RuntimeError):
    """Raised when a check fails."""


class SkipCheck(RuntimeError):
    """Raised when a check should be skipped."""


@dataclass
class CheckResult:
    name: str
    status: str
    duration: float
    details: str = ""


@dataclass
class Check:
    name: str
    modes: Sequence[str]
    func: Callable[["VerificationContext"], str]


@dataclass
class VerificationContext:
    mode: str
    json_output: bool
    changed_only: bool
    timeout_min: int
    net: bool
    update_marker_baseline: bool
    update_clippy_baseline: bool

    def __post_init__(self) -> None:
        ARTIFACTS_DIR.mkdir(parents=True, exist_ok=True)
        BASELINE_DIR.mkdir(parents=True, exist_ok=True)
        self.env = os.environ.copy()
        if "PKG_CONFIG_PATH" not in self.env:
            self.env["PKG_CONFIG_PATH"] = "/usr/lib/x86_64-linux-gnu/pkgconfig"
        self.changed_files = self._discover_changed_files()
        rustflags = self.env.get("RUSTFLAGS", "")
        cap_flag = "--cap-lints=warn"
        if cap_flag not in rustflags.split():
            rustflags = f"{rustflags} {cap_flag}".strip()
            self.env["RUSTFLAGS"] = rustflags
        self.result_payload: Dict[str, Dict[str, str]] = {}

    # Helpers -----------------------------------------------------------------
    def _discover_changed_files(self) -> List[str]:
        result = subprocess.run(
            ["git", "status", "--porcelain"],
            cwd=str(ROOT),
            capture_output=True,
            text=True,
            check=False,
        )
        files: List[str] = []
        for line in result.stdout.splitlines():
            if not line.strip():
                continue
            path = line[3:] if len(line) >= 4 else line
            files.append(path.strip())
        return files

    def has_changed_rust_sources(self) -> bool:
        return any(path.endswith(".rs") for path in self.changed_files)

    def run_command(self, name: str, command: Sequence[str], cwd: Optional[Path] = None) -> str:
        """Run command streaming output; raise on failure."""
        cwd = cwd or ROOT
        start = time.perf_counter()
        process = subprocess.Popen(
            command,
            cwd=str(cwd),
            env=self.env,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
        )
        output_lines: List[str] = []
        assert process.stdout is not None
        try:
            for line in process.stdout:
                sys.stdout.write(line)
                output_lines.append(line)
        finally:
            process.stdout.close()
        exit_code = process.wait()
        duration = time.perf_counter() - start
        tail = "".join(output_lines)[-2000:]
        self.result_payload[name] = {
            "command": " ".join(command),
            "duration_sec": f"{duration:.2f}",
            "exit_code": str(exit_code),
            "output_tail": tail,
        }
        if exit_code != 0:
            raise CheckFailure(f"Command {' '.join(command)} exited with code {exit_code}")
        return tail

    def apply_rust_excludes(self, args: List[str]) -> List[str]:
        for pkg in RUST_EXCLUDE_PACKAGES:
            args.extend(["--exclude", pkg])
        return args


# ----------------------------------------------------------------------------
# Utilities
# ----------------------------------------------------------------------------

def format_duration(seconds: float) -> str:
    return f"{seconds:.2f}s"


def discover_language_stacks() -> Dict[str, bool]:
    stacks = {
        "rust": (ROOT / "Cargo.toml").exists(),
        "python": any((ROOT / name).exists() for name in ("pyproject.toml", "requirements.txt")),
        "javascript": (ROOT / "package.json").exists(),
        "go": (ROOT / "go.mod").exists(),
    }
    return stacks


def _env_positive_int(key: str, default: int, *, allow_zero: bool = False) -> int:
    value = os.environ.get(key)
    if value is None:
        return default
    try:
        parsed = int(value)
    except ValueError as exc:
        raise CheckFailure(f"Environment variable {key} must be an integer") from exc
    if allow_zero:
        if parsed < 0:
            raise CheckFailure(f"Environment variable {key} must be >= 0")
    else:
        if parsed <= 0:
            raise CheckFailure(f"Environment variable {key} must be > 0")
    return parsed


def _format_metric(metric: MetricValue) -> str:
    if metric.unit == "ratio":
        return f"{metric.value * 100:.2f}%"
    return f"{metric.value:.2f}{metric.unit}"


def _update_perf_status(status_value: str, suite_report, summary_text: str, artifacts: list[str], profile: str, probes):
    status_path = ROOT / "reports" / "status.json"
    status_path.parent.mkdir(parents=True, exist_ok=True)
    try:
        status_data = json.loads(status_path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        status_data = {"build": [], "tests": [], "perf": {}}
    except json.JSONDecodeError:
        status_data = {"build": [], "tests": [], "perf": {}}
    perf_data = status_data.setdefault("perf", {})
    perf_data["last_verify"] = {
        "status": status_value,
        "profile": profile,
        "generated_at": suite_report.generated_at,
        "summary": summary_text,
        "violations": suite_report.violated_budgets(),
        "artifacts": artifacts,
        "probes": probes,
    }
    status_path.write_text(json.dumps(status_data, indent=2, sort_keys=True), encoding="utf-8")


def check_perf_probes(ctx: VerificationContext) -> str:
    if ctx.mode not in {"full", "ci"}:
        raise SkipCheck("performance probes run only in full/ci modes")
    if shutil.which("k6") is None:
        raise CheckFailure("k6 binary is required for performance probes")

    profile_name = os.environ.get("SHELLDONE_PERF_PROFILE")
    if profile_name is None:
        profile_name = "ci" if ctx.mode == "ci" else "full"

    profile_cfg = get_profile(profile_name)
    if profile_name and profile_cfg is None:
        raise CheckFailure(f"Unknown performance profile: {profile_name}")

    if profile_cfg is not None:
        applied_env = apply_env_overrides(os.environ, profile_cfg.env_overrides, overwrite=False)
        for key, value in profile_cfg.env_overrides.items():
            ctx.env.setdefault(key, value)
            os.environ.setdefault(key, value)
    else:
        applied_env = os.environ

    artifacts_root = ROOT / "artifacts" / "perf"
    artifacts_root.mkdir(parents=True, exist_ok=True)
    log_path = artifacts_root / "agentd_perf.log"
    trials = _env_positive_int("SHELLDONE_PERF_TRIALS", profile_cfg.trials if profile_cfg else 3)
    warmup = _env_positive_int("SHELLDONE_PERF_WARMUP_SEC", profile_cfg.warmup_sec if profile_cfg else 10, allow_zero=True)
    policy_warmup = _env_positive_int("SHELLDONE_PERF_POLICY_WARMUP_SEC", profile_cfg.policy_warmup_sec if profile_cfg else warmup, allow_zero=True)
    runner = K6Runner()
    service = PerfProbeService(runner, artifacts_root)
    with tempfile.TemporaryDirectory(prefix="perf-agentd-") as tmp:
        state_dir = Path(tmp)
        process = None
        handle = None
        try:
            process, handle = start_agentd(
                state_dir=state_dir,
                log_path=log_path,
                cwd=ROOT,
                env=ctx.env,
            )
            try:
                wait_for_agentd("127.0.0.1:17717", timeout=30.0)
            except RuntimeError as err:
                raise CheckFailure(str(err)) from err
            specs = default_specs(
                trials,
                warmup,
                policy_warmup,
                env=applied_env,
            )
            suite_report = service.run_suite(specs)
            summary_dict = suite_report.to_dict()
            reports_perf_dir = ROOT / "reports" / "perf"
            service.export_suite_summary(reports_perf_dir / "summary.json", suite_report)
            prom_path = reports_perf_dir / "metrics.prom"
            prom_path.parent.mkdir(parents=True, exist_ok=True)
            prom_payload = render_prometheus_metrics(suite_report)
            if not prom_payload.strip():
                raise CheckFailure("Prometheus metrics export is empty")
            prom_path.write_text(prom_payload, encoding="utf-8")
            artifact_paths = list(dict.fromkeys(suite_report.artifact_paths))
            artifact_paths.append(str(log_path))
            artifact_paths.append(str(prom_path))
            ctx.result_payload["perf-probes"] = {
                "artifacts": artifact_paths,
                "generated_at": suite_report.generated_at,
                "violations": suite_report.violated_budgets(),
                "summary": summary_dict,
                "profile": profile_cfg.name if profile_cfg else profile_name,
                "metrics_prom": str(prom_path),
            }
            summaries = []
            for report in suite_report.reports:
                metrics = ", ".join(
                    f"{alias}={_format_metric(metric)}"
                    for alias, metric in sorted(report.aggregated.items())
                )
                summaries.append(f"{report.probe_id}: {metrics}")
            summary_text = "; ".join(summaries)
            probes_payload = summary_dict.get("probes", [])
            if suite_report.has_failures():
                _update_perf_status(
                    "fail",
                    suite_report,
                    summary_text,
                    artifact_paths,
                    profile_cfg.name if profile_cfg else profile_name,
                    probes_payload,
                )
                details = "; ".join(suite_report.violated_budgets())
                raise CheckFailure(f"Performance budgets violated: {details}")
            _update_perf_status(
                "ok",
                suite_report,
                summary_text,
                artifact_paths,
                profile_cfg.name if profile_cfg else profile_name,
                probes_payload,
            )
            return summary_text
        finally:
            if process is not None:
                stop_agentd(process, handle)


MARKER_REGEX = re.compile(r"(TODO|FIXME|XXX|\?\?\?)")
SKIP_MARKER_DIRS = {
    "target",
    "deps",
    "licenses",
    ".git",
    "docs/ROADMAP/notes",
    "docs/architecture/adr",
    "artifacts",
    "qa/baselines",
}
SKIP_MARKER_SUFFIXES = {".md", ".markdown", ".rst", ".txt"}
ALLOW_MARKER_BASENAMES = {"Makefile", "makefile"}
RUST_EXCLUDE_PACKAGES = [
    "cairo-sys-rs",
    "freetype",
    "fontconfig",
    "harfbuzz",
]
CLIPPY_BASELINE = BASELINE_DIR / "clippy.json"


# ----------------------------------------------------------------------------
# Checks
# ----------------------------------------------------------------------------

def check_markdown_links(_: VerificationContext) -> str:
    files = [
        ROOT / "README.md",
        ROOT / "CONTRIBUTING.md",
        ROOT / "AGENTS.md",
        ROOT / "docs/architecture/README.md",
        ROOT / "docs/ROADMAP/2025Q4.md",
    ]
    broken: List[str] = []
    link_re = re.compile(r"\[[^\]]+\]\(([^)]+)\)")
    for path in files:
        if not path.exists():
            raise CheckFailure(f"File {path} is missing for link validation")
        text = path.read_text(encoding="utf-8")
        lines = text.splitlines()
        for idx, line in enumerate(lines, start=1):
            for match in link_re.finditer(line):
                href = match.group(1).strip()
                if not href or href.startswith("http://") or href.startswith("https://"):
                    continue
                if href.startswith("#") or href.startswith("mailto:") or href.startswith("tel:"):
                    continue
                target = href.split("#", 1)[0]
                target_path = (path.parent / target).resolve()
                if not target_path.exists():
                    broken.append(f"{path.relative_to(ROOT)}:{idx} -> {href}")
    if broken:
        raise CheckFailure("Broken local links detected:\n" + "\n".join(broken))
    return "All local links are valid"


def _section_lines(text: str, header: str) -> List[str]:
    lines = text.splitlines()
    capture = False
    section: List[str] = []
    header_line = f"## {header}"
    for line in lines:
        if line.strip() == header_line:
            capture = True
            continue
        if capture and line.startswith("## "):
            break
        if capture:
            section.append(line)
    if not section:
        raise CheckFailure(f"Section '{header}' is missing in todo.machine.md")
    return section


def _extract_yaml_blocks(section_lines: List[str]) -> List[str]:
    blocks: List[str] = []
    collecting = False
    buf: List[str] = []
    for line in section_lines:
        stripped = line.strip()
        if not collecting and stripped.startswith("```yaml"):
            collecting = True
            buf = []
            continue
        if collecting and stripped.startswith("```"):
            blocks.append("\n".join(buf).strip())
            collecting = False
            continue
        if collecting:
            buf.append(line)
    if collecting:
        raise CheckFailure("Unclosed yaml block in todo.machine.md")
    return [block for block in blocks if block]


def _check_required_fields(name: str, data: Dict[str, object], required: Iterable[str]) -> None:
    missing = [field for field in required if field not in data]
    if missing:
        raise CheckFailure(f"{name}: missing fields {', '.join(missing)}")


def check_todo_machine(_: VerificationContext) -> str:
    path = ROOT / "todo.machine.md"
    if not path.exists():
        raise CheckFailure("todo.machine.md is missing")
    text = path.read_text(encoding="utf-8")
    program_blocks = _extract_yaml_blocks(_section_lines(text, "Program"))
    if len(program_blocks) != 1:
        raise CheckFailure("Program section must contain exactly one yaml block")
    program = yaml.safe_load(program_blocks[0])
    _check_required_fields(
        "Program",
        program,
        [
            "program",
            "updated_at",
            "program_id",
            "name",
            "objectives",
            "kpis",
            "progress_pct",
            "health",
            "milestones",
            "policies",
        ],
    )
    if not isinstance(program["progress_pct"], int):
        raise CheckFailure("Program.progress_pct must be an integer")
    if program["progress_pct"] < 0 or program["progress_pct"] > 100:
        raise CheckFailure("Program.progress_pct must be between 0 and 100")

    epics_blocks = _extract_yaml_blocks(_section_lines(text, "Epics"))
    if not epics_blocks:
        raise CheckFailure("Epics section is empty")
    epics = []
    for block in epics_blocks:
        epic = yaml.safe_load(block)
        _check_required_fields(
            f"Epic {epic.get('id','<unknown>')}",
            epic,
            [
                "id",
                "title",
                "type",
                "status",
                "priority",
                "size_points",
                "scope_paths",
                "spec",
                "budgets",
                "risks",
                "dependencies",
                "big_tasks_planned",
                "progress_pct",
                "health",
                "tests_required",
                "verify_commands",
                "docs_updates",
                "artifacts",
                "audit",
            ],
        )
        if epic["type"] != "epic":
            raise CheckFailure(f"{epic['id']}: type must be 'epic'")
        if epic["priority"] not in {"P0", "P1", "P2", "P3"}:
            raise CheckFailure(f"{epic['id']}: unknown priority {epic['priority']}")
        if epic["status"] not in {"planned", "in_progress", "blocked", "at_risk", "review", "done"}:
            raise CheckFailure(f"{epic['id']}: unknown status {epic['status']}")
        if epic["health"] not in {"green", "yellow", "red"}:
            raise CheckFailure(f"{epic['id']}: unknown health {epic['health']}")
        if epic["size_points"] not in {8, 13, 20, 40}:
            raise CheckFailure(f"{epic['id']}: size_points must be one of 8, 13, 20, 40")
        if not isinstance(epic["scope_paths"], list) or not epic["scope_paths"]:
            raise CheckFailure(f"{epic['id']}: scope_paths must be a non-empty list")
        for pattern in epic["scope_paths"]:
            matches = list(ROOT.glob(pattern))
            if not matches:
                raise CheckFailure(f"{epic['id']}: scope pattern '{pattern}' matched nothing")
        epics.append(epic)

    big_task_blocks = _extract_yaml_blocks(_section_lines(text, "Big Tasks"))
    if not big_task_blocks:
        raise CheckFailure("Big Tasks section is empty")
    big_tasks = []
    for block in big_task_blocks:
        task = yaml.safe_load(block)
        _check_required_fields(
            f"Big task {task.get('id','<unknown>')}",
            task,
            [
                "id",
                "title",
                "type",
                "status",
                "priority",
                "size_points",
                "parent_epic",
                "scope_paths",
                "spec",
                "budgets",
                "risks",
                "dependencies",
                "progress_pct",
                "health",
                "tests_required",
                "verify_commands",
                "docs_updates",
                "artifacts",
                "audit",
            ],
        )
        if task["type"] not in {"feature", "perf", "migration", "refactor", "test", "doc", "ops", "research"}:
            raise CheckFailure(f"{task['id']}: unknown type {task['type']}")
        if task["size_points"] not in {5, 8, 13}:
            raise CheckFailure(f"{task['id']}: size_points must be 5, 8, or 13")
        if task["priority"] not in {"P0", "P1", "P2", "P3"}:
            raise CheckFailure(f"{task['id']}: unknown priority {task['priority']}")
        if task["status"] not in {"planned", "in_progress", "blocked", "at_risk", "review", "done"}:
            raise CheckFailure(f"{task['id']}: unknown status {task['status']}")
        if task["health"] not in {"green", "yellow", "red"}:
            raise CheckFailure(f"{task['id']}: unknown health {task['health']}")
        if not isinstance(task["scope_paths"], list) or not task["scope_paths"]:
            raise CheckFailure(f"{task['id']}: scope_paths must be a non-empty list")
        for pattern in task["scope_paths"]:
            matches = list(ROOT.glob(pattern))
            if not matches:
                raise CheckFailure(f"{task['id']}: scope pattern '{pattern}' matched nothing")
        big_tasks.append(task)

    epic_map = {epic["id"]: epic for epic in epics}
    for task in big_tasks:
        if task["parent_epic"] not in epic_map:
            raise CheckFailure(f"{task['id']}: parent_epic {task['parent_epic']} does not exist")

    # progress consistency ----------------------------------------------------
    for epic in epics:
        related = [task for task in big_tasks if task["parent_epic"] == epic["id"]]
        if not related:
            raise CheckFailure(f"{epic['id']}: has no big tasks")
        total_points = sum(task["size_points"] for task in related)
        weighted = sum(task["size_points"] * task["progress_pct"] for task in related)
        expected = round(weighted / total_points) if total_points else 0
        if abs(expected - epic["progress_pct"]) > 1:
            raise CheckFailure(
                f"{epic['id']}: progress_pct {epic['progress_pct']} is inconsistent with big tasks ({expected})"
            )
    total_epic_points = sum(epic["size_points"] for epic in epics)
    weighted_epic = sum(epic["size_points"] * epic["progress_pct"] for epic in epics)
    expected_program = round(weighted_epic / total_epic_points) if total_epic_points else 0
    if abs(expected_program - program["progress_pct"]) > 1:
        raise CheckFailure(
            f"Program.progress_pct {program['progress_pct']} in todo.machine.md differs from computed {expected_program}"
        )
    return f"Program {program['name']} is aligned with {len(epics)} epics and {len(big_tasks)} big tasks"


def check_architecture_doc(_: VerificationContext) -> str:
    path = ROOT / "docs/architecture/README.md"
    if not path.exists():
        raise CheckFailure("docs/architecture/README.md is missing")
    text = path.read_text(encoding="utf-8")
    required_headers = [
        "# Shelldone Architecture Overview",
        "## Core Principles",
        "## Thematic Specifications",
        "## Roadmap",
    ]
    for header in required_headers:
        if header not in text:
            raise CheckFailure(f"Missing heading '{header}' in architecture overview")
    references = [
        "docs/architecture/customization-and-plugins.md",
        "docs/architecture/ai-integration.md",
        "docs/architecture/animation-framework.md",
        "docs/architecture/perf-budget.md",
    ]
    missing_refs = [ref for ref in references if ref not in text]
    if missing_refs:
        raise CheckFailure("Architecture overview missing references: " + ", ".join(missing_refs))
    return "Architecture overview contains required sections"


def _parse_markdown_table(lines: List[str]) -> List[Dict[str, str]]:
    if len(lines) < 2:
        raise CheckFailure("Table must contain header and data rows")
    header = [cell.strip() for cell in lines[0].strip().strip("|").split("|")]
    data_rows = lines[2:]  # skip separator row
    entries: List[Dict[str, str]] = []
    for row in data_rows:
        row = row.strip()
        if not row or not row.startswith("|"):
            continue
        cells = [cell.strip() for cell in row.strip().strip("|").split("|")]
        if len(cells) != len(header):
            raise CheckFailure("Table row has incorrect number of columns")
        entries.append(dict(zip(header, cells)))
    return entries


def check_roadmap(_: VerificationContext) -> str:
    path = ROOT / "docs/ROADMAP/2025Q4.md"
    text = path.read_text(encoding="utf-8")
    lines = text.splitlines()

    def extract_table(title: str) -> List[str]:
        start = None
        for idx, line in enumerate(lines):
            if line.strip() == title:
                start = idx + 1
                break
        if start is None:
            raise CheckFailure(f"Roadmap section '{title}' is missing")
        collected: List[str] = []
        for line in lines[start:]:
            if line.startswith("## "):
                break
            if line.strip().startswith("|"):
                collected.append(line)
        return collected

    epic_entries = _parse_markdown_table(extract_table("## Epic Map"))
    task_entries = _parse_markdown_table(extract_table("## Task Table"))

    todo_text = (ROOT / "todo.machine.md").read_text(encoding="utf-8")
    epics = [yaml.safe_load(block) for block in _extract_yaml_blocks(_section_lines(todo_text, "Epics"))]
    big_tasks = [yaml.safe_load(block) for block in _extract_yaml_blocks(_section_lines(todo_text, "Big Tasks"))]

    epic_map = {entry["Epic ID"]: entry for entry in epic_entries}
    for epic in epics:
        epic_id = epic["id"]
        if epic_id not in epic_map:
            raise CheckFailure(f"Roadmap does not list epic {epic_id}")
        entry = epic_map[epic_id]
        if entry.get("Status") != epic["status"]:
            raise CheckFailure(f"Roadmap status mismatch for {epic_id} (found {entry.get('Status')})")
        scope_text = entry.get("Scope")
        expected_scope = ", ".join(epic["scope_paths"])
        if scope_text != expected_scope:
            raise CheckFailure(f"Roadmap scope mismatch for {epic_id}")

    task_map = {entry["Task ID"]: entry for entry in task_entries}
    for task in big_tasks:
        task_id = task["id"]
        if task_id not in task_map:
            raise CheckFailure(f"Roadmap does not list task {task_id}")
        entry = task_map[task_id]
        if entry.get("Epic") != task["parent_epic"]:
            raise CheckFailure(f"Roadmap task {task_id} references epic {entry.get('Epic')} instead of {task['parent_epic']}")
        if entry.get("Type") != task["type"]:
            raise CheckFailure(f"Roadmap task {task_id} type mismatch")
        if entry.get("Size") != str(task["size_points"]):
            raise CheckFailure(f"Roadmap task {task_id} size mismatch")
        if entry.get("Status") != task["status"]:
            raise CheckFailure(f"Roadmap task {task_id} status mismatch")
    return f"Roadmap lists {len(epic_entries)} epics and {len(task_entries)} tasks"


def _collect_markers() -> Dict[str, Counter]:
    files = subprocess.run(["git", "ls-files"], check=True, capture_output=True, text=True, cwd=ROOT)
    result: Dict[str, Counter] = defaultdict(Counter)
    for rel_path in files.stdout.strip().splitlines():
        if not rel_path:
            continue
        for skip in SKIP_MARKER_DIRS:
            if rel_path.startswith(skip):
                break
        else:
            path = ROOT / rel_path
            suffix = path.suffix.lower()
            if suffix in SKIP_MARKER_SUFFIXES and path.name not in ALLOW_MARKER_BASENAMES:
                continue
            try:
                data = path.read_text(encoding="utf-8")
            except UnicodeDecodeError:
                continue
            for line in data.splitlines():
                match = MARKER_REGEX.search(line)
                if match:
                    token = match.group(1)
                    snippet = line.strip()
                    key = f"{token}:{snippet}"
                    result[rel_path][key] += 1
    return result


def check_banned_markers(ctx: VerificationContext) -> str:
    markers = _collect_markers()
    if ctx.update_marker_baseline:
        payload = {
            "version": 1,
            "generated_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "files": [
                {"path": path, "markers": sorted(({"token": entry.split(":", 1)[0], "text": entry.split(":", 1)[1], "count": count} for entry, count in counter.items()), key=lambda x: x["text"])}
                for path, counter in sorted(markers.items())
            ],
        }
        MARKER_BASELINE.write_text(json.dumps(payload, indent=2, ensure_ascii=True) + "\n", encoding="utf-8")
        return "Marker baseline updated"
    if not MARKER_BASELINE.exists():
        raise CheckFailure("Marker baseline missing. Run scripts/verify.py --update-marker-baseline")
    baseline = json.loads(MARKER_BASELINE.read_text(encoding="utf-8"))
    expected: Dict[str, Counter] = {}
    for entry in baseline.get("files", []):
        counter = Counter({f"{item['token']}:{item['text']}": item["count"] for item in entry.get("markers", [])})
        expected[entry["path"]] = counter
    unexpected: List[str] = []
    missing: List[str] = []
    for path, counter in markers.items():
        diff = counter - expected.get(path, Counter())
        for token, count in diff.items():
            unexpected.append(f"{path}: +{count} {token.split(':', 1)[1][:80]}")
    for path, counter in expected.items():
        diff = counter - markers.get(path, Counter())
        for token, count in diff.items():
            missing.append(f"{path}: -{count} {token.split(':', 1)[1][:80]}")
    messages = []
    if unexpected:
        messages.append("New forbidden markers:\n" + "\n".join(unexpected[:20]))
    if missing:
        messages.append("Markers removed without baseline update:\n" + "\n".join(missing[:20]))
    if messages:
        raise CheckFailure("; ".join(messages))
    return f"Forbidden markers under control (total {sum(sum(c.values()) for c in markers.values())})"


def check_agent_adapters(ctx: VerificationContext) -> str:
    cmd = [sys.executable, str(ROOT / "scripts" / "agentd.py"), "smoke"]
    ctx.run_command("agent-smoke", cmd)
    return "Agent adapters emit structured errors without dependencies"


def check_rust_fmt(ctx: VerificationContext) -> str:
    if not ctx.has_changed_rust_sources():
        raise SkipCheck("no .rs changes detected")
    ctx.run_command("rust-fmt", ["cargo", "+nightly", "fmt", "--all", "--", "--check"])
    return "cargo fmt --all -- --check"


def check_rust_clippy(ctx: VerificationContext) -> str:
    args: List[str] = [
        "cargo",
        "clippy",
        "--workspace",
        "--all-features",
        "--message-format=json",
    ]
    ctx.apply_rust_excludes(args)
    args.extend(["--", "--no-deps"])

    start = time.perf_counter()
    process = subprocess.Popen(
        args,
        cwd=str(ROOT),
        env=ctx.env,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )

    warnings: List[Dict[str, object]] = []
    assert process.stdout is not None
    for line in process.stdout:
        line = line.strip()
        if not line:
            continue
        try:
            payload = json.loads(line)
        except json.JSONDecodeError:
            sys.stdout.write(line + "\n")
            continue

        if payload.get("reason") != "compiler-message":
            message = payload.get("message")
            if isinstance(message, str):
                sys.stdout.write(message)
                if not message.endswith("\n"):
                    sys.stdout.write("\n")
            continue

        msg = payload.get("message", {})
        rendered = msg.get("rendered")
        if rendered:
            sys.stdout.write(rendered)

        code = (msg.get("code") or {}).get("code")
        level = msg.get("level")
        spans = msg.get("spans", [])
        if code and code.startswith("clippy::") and level == "warning":
            primary = next((span for span in spans if span.get("is_primary")), spans[0] if spans else None)
            warnings.append(
                {
                    "code": code,
                    "message": msg.get("message", ""),
                    "file": primary.get("file_name") if primary else "",
                    "line": primary.get("line_start") if primary else 0,
                }
            )
        elif level == "error":
            # An actual clippy error occurred
            pass

    exit_code = process.wait()
    duration = time.perf_counter() - start
    ctx.result_payload["rust-clippy"] = {
        "command": " ".join(args),
        "duration_sec": f"{duration:.2f}",
        "warning_count": str(len(warnings)),
    }
    if exit_code != 0:
        raise CheckFailure(f"cargo clippy exited with code {exit_code}")

    # Baseline handling ------------------------------------------------------
    warnings_sorted = sorted(
        warnings,
        key=lambda w: (w["code"], w["file"], int(w.get("line", 0)), w["message"]),
    )

    if ctx.update_clippy_baseline:
        payload = {
            "version": 1,
            "generated_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "warnings": warnings_sorted,
        }
        CLIPPY_BASELINE.write_text(json.dumps(payload, indent=2, ensure_ascii=True) + "\n", encoding="utf-8")
        return f"Clippy baseline updated ({len(warnings_sorted)} warnings)"

    if not CLIPPY_BASELINE.exists():
        raise CheckFailure("Clippy baseline missing. Run scripts/verify.py --update-clippy-baseline")

    baseline = json.loads(CLIPPY_BASELINE.read_text(encoding="utf-8"))
    baseline_warnings = baseline.get("warnings", [])
    baseline_sorted = sorted(
        baseline_warnings,
        key=lambda w: (w["code"], w["file"], int(w.get("line", 0)), w["message"]),
    )

    current_set = {json.dumps(w, sort_keys=True) for w in warnings_sorted}
    baseline_set = {json.dumps(w, sort_keys=True) for w in baseline_sorted}

    new_warnings = current_set - baseline_set
    resolved_warnings = baseline_set - current_set

    if new_warnings:
        sample = [json.loads(item) for item in sorted(new_warnings)[:5]]
        details = "\n".join(
            f"{w['code']} {w['file']}:{w['line']} — {w['message']}" for w in sample
        )
        raise CheckFailure("New Clippy warnings detected:\n" + details)

    if resolved_warnings:
        sample = [json.loads(item) for item in sorted(resolved_warnings)[:5]]
        details = "\n".join(
            f"{w['code']} {w['file']}:{w['line']} — {w['message']}" for w in sample
        )
        raise CheckFailure("Clippy baseline is outdated, update it:\n" + details)
    return f"Clippy warnings: {len(warnings_sorted)} (match baseline)"


def check_rust_test(ctx: VerificationContext) -> str:
    args: List[str] = ["cargo", "test", "--workspace", "--all-features"]
    ctx.apply_rust_excludes(args)
    ctx.run_command("rust-test", args)
    return "cargo test --workspace --all-features"


def check_rust_nextest(ctx: VerificationContext) -> str:
    args: List[str] = ["cargo", "nextest", "run", "--workspace"]
    ctx.apply_rust_excludes(args)
    ctx.run_command("rust-nextest", args)
    return "cargo nextest run --workspace"


def check_rust_doc(ctx: VerificationContext) -> str:
    args: List[str] = ["cargo", "doc", "--workspace", "--no-deps"]
    ctx.apply_rust_excludes(args)
    ctx.run_command("rust-doc", args)
    return "cargo doc --no-deps"


def check_python(ctx: VerificationContext) -> str:
    stacks = discover_language_stacks()
    if not stacks["python"]:
        raise SkipCheck("python stack not detected")
    ctx.run_command("python-compile", ["python3", "-m", "compileall", str(ROOT)])
    tests_dir = ROOT / "scripts" / "tests"
    if tests_dir.exists():
        ctx.run_command(
            "python-unittest",
            ["python3", "-m", "unittest", "discover", "-s", str(tests_dir)],
        )
    return "python compileall + unittest"


def check_js(ctx: VerificationContext) -> str:
    stacks = discover_language_stacks()
    if not stacks["javascript"]:
        raise SkipCheck("javascript stack not detected")
    ctx.run_command("npm-lint", ["npm", "run", "lint"])
    ctx.run_command("npm-test", ["npm", "test"])
    return "npm lint + test"


def check_go(ctx: VerificationContext) -> str:
    stacks = discover_language_stacks()
    if not stacks["go"]:
        raise SkipCheck("go stack not detected")
    ctx.run_command("go-fmt", ["go", "fmt", "./..."])
    ctx.run_command("go-vet", ["go", "vet", "./..."])
    ctx.run_command("go-test", ["go", "test", "./..."])
    return "go fmt/vet/test"


CHECKS: List[Check] = [
    Check("docs-links", ("fast", "prepush", "full", "ci"), check_markdown_links),
    Check("todo-machine", ("fast", "prepush", "full", "ci"), check_todo_machine),
    Check("architecture-doc", ("fast", "prepush", "full", "ci"), check_architecture_doc),
    Check("roadmap", ("prepush", "full", "ci"), check_roadmap),
    Check("banned-markers", ("fast", "prepush", "full", "ci"), check_banned_markers),
    Check("agents", ("prepush", "full", "ci"), check_agent_adapters),
    Check("rust-fmt", ("fast", "prepush", "full", "ci"), check_rust_fmt),
    Check("rust-clippy", ("fast", "prepush", "full", "ci"), check_rust_clippy),
    Check("rust-test", ("fast", "prepush", "full", "ci"), check_rust_test),
    Check("rust-nextest", ("prepush", "full", "ci"), check_rust_nextest),
    Check("rust-doc", ("full", "ci"), check_rust_doc),
    Check("perf-probes", ("full", "ci"), check_perf_probes),
    Check("python", ("full", "ci"), check_python),
    Check("javascript", ("full", "ci"), check_js),
    Check("go", ("full", "ci"), check_go),
]


def run_checks(ctx: VerificationContext) -> List[CheckResult]:
    results: List[CheckResult] = []
    for check in CHECKS:
        if ctx.mode not in check.modes:
            continue
        start = time.perf_counter()
        try:
            details = check.func(ctx)
            status = PASS
        except SkipCheck as skip:
            status = SKIP
            details = str(skip)
        except CheckFailure as failure:
            status = FAIL
            details = str(failure)
        except Exception as unexpected:
            status = FAIL
            details = f"Unhandled exception: {unexpected}"
        duration = time.perf_counter() - start
        results.append(CheckResult(check.name, status, duration, details))
    return results


def print_summary(results: Sequence[CheckResult]) -> None:
    print("\nSummary:")
    header = ["Check", "Status", "Time", "Details"]
    rows = [header]
    for result in results:
        rows.append(
            [
                result.name,
                result.status,
                format_duration(result.duration),
                (result.details or "").replace("\n", ", ")[:120],
            ]
        )
    widths = [max(len(row[idx]) for row in rows) for idx in range(len(header))]
    for idx, row in enumerate(rows):
        line = "  ".join(col.ljust(widths[i]) for i, col in enumerate(row))
        if idx == 0:
            print(line)
            print("  ".join("-" * widths[i] for i in range(len(header))))
        else:
            print(line)


def results_to_json(results: Sequence[CheckResult]) -> Dict[str, object]:
    return {
        "summary": {
            "status": "fail" if any(r.status == FAIL for r in results) else "pass",
            "mode": args.mode,
        },
        "checks": [
            {
                "name": r.name,
                "status": r.status,
                "duration_sec": round(r.duration, 2),
                "details": r.details,
            }
            for r in results
        ],
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Shelldone QA orchestrator")
    parser.add_argument("--mode", choices=["fast", "prepush", "full", "ci"], default="prepush")
    parser.add_argument("--json", choices=["0", "1"], default="0")
    parser.add_argument("--changed-only", choices=["0", "1"], default="0")
    parser.add_argument("--timeout-min", default="0")
    parser.add_argument("--net", choices=["0", "1"], default="0")
    parser.add_argument("--update-marker-baseline", action="store_true")
    parser.add_argument("--update-clippy-baseline", action="store_true")
    parser.add_argument("--list-checks", action="store_true")
    return parser.parse_args()


def main() -> int:
    global args
    args = parse_args()
    if args.list_checks:
        for check in CHECKS:
            print(f"{check.name}: {'/'.join(check.modes)}")
        return 0
    ctx = VerificationContext(
        mode=args.mode,
        json_output=args.json == "1",
        changed_only=args.changed_only == "1",
        timeout_min=int(args.timeout_min),
        net=args.net == "1",
        update_marker_baseline=args.update_marker_baseline,
        update_clippy_baseline=args.update_clippy_baseline,
    )
    if ctx.update_marker_baseline:
        message = check_banned_markers(ctx)
        print(message)
        return 0
    results = run_checks(ctx)
    print_summary(results)
    payload = {
        "mode": args.mode,
        "status": "fail" if any(r.status == FAIL for r in results) else "pass",
        "checks": [
            {
                "name": r.name,
                "status": r.status,
                "duration_sec": round(r.duration, 2),
                "details": r.details,
            }
            for r in results
        ],
        "commands": ctx.result_payload,
    }
    (ARTIFACTS_DIR / "summary.json").write_text(json.dumps(payload, indent=2, ensure_ascii=True) + "\n", encoding="utf-8")
    if ctx.json_output:
        print(json.dumps(payload, indent=2, ensure_ascii=True))
    return 1 if any(r.status == FAIL for r in results) else 0


if __name__ == "__main__":
    sys.exit(main())

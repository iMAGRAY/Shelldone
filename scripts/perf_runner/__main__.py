from __future__ import annotations

import argparse
import json
import os
import sys
import tempfile
from pathlib import Path
from typing import Iterable, Mapping, Optional

from .adapters.k6_runner import K6Runner
from .app.service import PerfProbeService
from .domain.probe import ProbeSpec, ProbeTrialResult
from .domain.value_objects import MetricValue
from .infra.agentd import start_agentd, stop_agentd, wait_for_agentd
from .ports.runner import ProbeExecutionError, ProbeRunnerPort
from .specs import build_policy_spec, build_utif_exec_spec, build_experience_hub_spec, default_specs
from .profiles import apply_env_overrides, get_profile
from .reporting import render_prometheus_metrics


ROOT = Path(__file__).resolve().parent.parent.parent


class StubRunner(ProbeRunnerPort):
    """Deterministic runner for tests and dry-runs."""

    def run(
        self,
        spec: ProbeSpec,
        trial_index: int,
        output_dir: Path,
    ) -> ProbeTrialResult:
        output_dir.mkdir(parents=True, exist_ok=True)
        metrics: dict[str, MetricValue] = {}
        for definition in spec.metrics:
            if definition.unit == "ms":
                value = 10.0
            else:
                value = 0.001
            metrics[definition.alias] = MetricValue(
                alias=definition.alias,
                value=value,
                unit=definition.unit,
            )
        summary_path = output_dir / f"{spec.summary_prefix}_trial{trial_index + 1}_stub.json"
        summary_payload = {
            "probe_id": spec.probe_id,
            "trial": trial_index + 1,
            "metrics": {
                alias: {"value": metric.value, "unit": metric.unit}
                for alias, metric in metrics.items()
            },
        }
        summary_path.write_text(json.dumps(summary_payload, indent=2), encoding="utf-8")
        return ProbeTrialResult(
            trial_index=trial_index,
            metrics=metrics,
            summary_path=str(summary_path),
        )


def _resolve_specs(
    requested: Optional[Iterable[str]],
    trials: int,
    warmup_seconds: int,
    policy_warmup_seconds: int,
    env: Mapping[str, str],
) -> list[ProbeSpec]:
    if not requested:
        return default_specs(trials, warmup_seconds, policy_warmup_seconds, env=env)
    specs: list[ProbeSpec] = []
    seen: set[str] = set()
    for name in requested:
        if name in seen:
            continue
        seen.add(name)
        if name == "utif_exec":
            specs.append(build_utif_exec_spec(trials, warmup_seconds, env=env))
        elif name == "policy_perf":
            specs.append(build_policy_spec(trials, policy_warmup_seconds, env=env))
        elif name == "experience_hub":
            specs.append(build_experience_hub_spec(trials, warmup_seconds, env=env))
        else:  # pragma: no cover - argparse restricts choices
            raise ValueError(f"Unknown probe {name}")
    return specs


def _format_metric(metric: MetricValue) -> str:
    if metric.unit == "ratio":
        return f"{metric.value * 100:.2f}%"
    return f"{metric.value:.2f}{metric.unit}"


def _print_summary(reports: Iterable) -> None:
    for report in reports:
        metrics = ", ".join(
            f"{alias}={_format_metric(metric)}"
            for alias, metric in sorted(report.aggregated.items())
        )
        print(f"{report.probe_id}: {metrics}")


def _append_agentd_artifact(artifact_paths: list[str], path: Path) -> None:
    if str(path) not in artifact_paths:
        artifact_paths.append(str(path))


def _flag_passed(argv_list: Optional[list[str]], flag: str) -> bool:
    if argv_list is None:
        return False
    prefix = f"{flag}="
    return any(arg == flag or arg.startswith(prefix) for arg in argv_list)


def run_cli(argv: Optional[Iterable[str]] = None) -> int:
    parser = argparse.ArgumentParser(prog="perf_runner", description="Shelldone performance probes")
    subparsers = parser.add_subparsers(dest="command", required=True)

    run_parser = subparsers.add_parser("run", help="Execute configured probes")
    run_parser.add_argument("--runner", choices=["k6", "stub"], default="k6")  # qa:allow-realness
    run_parser.add_argument("--artifacts-dir", default="artifacts/perf")
    run_parser.add_argument("--profile", choices=["dev", "ci", "full", "staging", "prod"], default=None)
    run_parser.add_argument("--trials", type=int, default=int(os.getenv("SHELLDONE_PERF_TRIALS", "3")))
    run_parser.add_argument("--warmup-sec", type=int, default=int(os.getenv("SHELLDONE_PERF_WARMUP_SEC", "10")))
    run_parser.add_argument(
        "--policy-warmup-sec",
        type=int,
        default=int(os.getenv("SHELLDONE_PERF_POLICY_WARMUP_SEC", os.getenv("SHELLDONE_PERF_WARMUP_SEC", "10"))),
    )
    run_parser.add_argument("--listen", default="127.0.0.1:17717")
    run_parser.add_argument("--grpc-listen", default="127.0.0.1:17718")
    run_parser.add_argument("--state-dir", type=Path, default=None)
    run_parser.add_argument("--agentd-log", default="agentd_perf.log")
    run_parser.add_argument("--agentd-timeout", type=float, default=30.0)
    run_parser.add_argument("--summary-path", type=Path, default=None, help="Optional path for additional summary export")
    run_parser.add_argument("--prom-path", type=Path, default=None, help="Optional OpenMetrics export path")
    run_parser.add_argument("--probe", dest="probes", action="append", choices=["utif_exec", "policy_perf", "experience_hub"], help="Run only the specified probe(s)")
    run_parser.add_argument("--no-agentd", action="store_true", help="Do not start shelldone-agentd")

    argv_list = list(argv) if argv is not None else None
    args = parser.parse_args(argv_list)

    if args.command == "run":
        return _handle_run(args, argv_list)
    return 0


def _handle_run(args, argv_list) -> int:
    runner: ProbeRunnerPort = K6Runner() if args.runner == "k6" else StubRunner()
    artifacts_root = Path(args.artifacts_dir)
    artifacts_root.mkdir(parents=True, exist_ok=True)
    service = PerfProbeService(runner, artifacts_root)

    profile_cfg = get_profile(args.profile)
    env_for_specs = os.environ.copy()
    runner_env = os.environ.copy()
    if profile_cfg is not None:
        env_for_specs = apply_env_overrides(env_for_specs, profile_cfg.env_overrides)
        runner_env = apply_env_overrides(runner_env, profile_cfg.env_overrides)
        for key, value in profile_cfg.env_overrides.items():
            os.environ.setdefault(key, value)
        if not _flag_passed(argv_list, "--trials"):
            args.trials = profile_cfg.trials
        if not _flag_passed(argv_list, "--warmup-sec"):
            args.warmup_sec = profile_cfg.warmup_sec
        if not _flag_passed(argv_list, "--policy-warmup-sec"):
            args.policy_warmup_sec = profile_cfg.policy_warmup_sec

    specs = _resolve_specs(
        args.probes,
        args.trials,
        args.warmup_sec,
        args.policy_warmup_sec,
        env_for_specs,
    )

    process = handle = None
    temp_state: Optional[tempfile.TemporaryDirectory] = None
    state_dir: Optional[Path] = None
    log_path = artifacts_root / args.agentd_log

    start_agentd_required = args.runner == "k6" and not args.no_agentd

    try:
        if start_agentd_required:
            if args.state_dir is not None:
                state_dir = args.state_dir
                state_dir.mkdir(parents=True, exist_ok=True)
            else:
                temp_state = tempfile.TemporaryDirectory(prefix="perf-agentd-cli-")
                state_dir = Path(temp_state.name)
            process, handle = start_agentd(
                state_dir=state_dir,
                log_path=log_path,
                cwd=ROOT,
                env=runner_env.copy(),
                listen=args.listen,
                grpc_listen=args.grpc_listen,
            )
            try:
                wait_for_agentd(args.listen, timeout=args.agentd_timeout)
            except RuntimeError as err:
                return _abort(f"agentd startup failed: {err}")

        try:
            suite_report = service.run_suite(specs)
        except ProbeExecutionError as err:
            return _abort(f"probe execution failed: {err}")

        summary_dict = suite_report.to_dict()
        reports_perf_dir = ROOT / "reports" / "perf"
        service.export_suite_summary(reports_perf_dir / "summary.json", suite_report)
        if args.summary_path is not None:
            service.export_suite_summary(args.summary_path, suite_report)

        prom_path = args.prom_path or (ROOT / "reports" / "perf" / "metrics.prom")
        prom_path.parent.mkdir(parents=True, exist_ok=True)
        prom_path.write_text(render_prometheus_metrics(suite_report), encoding="utf-8")

        artifact_paths = list(dict.fromkeys(suite_report.artifact_paths))
        if start_agentd_required:
            _append_agentd_artifact(artifact_paths, log_path)

        _print_summary(suite_report.reports)
        summaries = []
        for report in suite_report.reports:
            metrics = ", ".join(
                f"{alias}={_format_metric(metric)}"
                for alias, metric in sorted(report.aggregated.items())
            )
            summaries.append(f"{report.probe_id}: {metrics}")
        summary_text = "; ".join(summaries)
        if suite_report.has_failures():
            violations = "; ".join(suite_report.violated_budgets())
            return _abort(f"Performance budgets violated: {violations}")

        print(f"Artifacts: {', '.join(artifact_paths)}")
        if args.summary_path is not None:
            print(f"Summary exported to {args.summary_path}")
        return 0
    finally:
        if process is not None and handle is not None:
            stop_agentd(process, handle)
        if temp_state is not None:
            temp_state.cleanup()


def _abort(message: str) -> int:
    print(message, file=sys.stderr)
    return 1


def main() -> None:
    sys.exit(run_cli())


if __name__ == "__main__":
    main()

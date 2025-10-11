from __future__ import annotations

import os
from pathlib import Path
from typing import List

from .domain.probe import ProbeSpec
from .domain.value_objects import MetricBudget, MetricDefinition, ProbeScript

ROOT = Path(__file__).resolve().parent.parent.parent


def build_utif_exec_spec(trials: int, warmup_seconds: int, *, env: dict | None = None) -> ProbeSpec:
    env = env or os.environ
    script = ProbeScript(ROOT / "scripts/perf/utif_exec.js")
    extra_env = {
        "SHELLDONE_PERF_DURATION": env.get("SHELLDONE_PERF_DURATION", "30s"),
        "SHELLDONE_PERF_RATE": env.get("SHELLDONE_PERF_RATE", "200"),
        "SHELLDONE_PERF_VUS": env.get("SHELLDONE_PERF_VUS", "50"),
        "SHELLDONE_PERF_MAX_VUS": env.get("SHELLDONE_PERF_MAX_VUS", "100"),
        "SHELLDONE_PERF_WARMUP_SEC": env.get("SHELLDONE_PERF_WARMUP_SEC", "0"),
    }
    return ProbeSpec(
        probe_id="utif_exec",
        label="UTIF Σ agent.exec load",
        script=script,
        metrics=[
            MetricDefinition("utif_exec_latency", "p(95)", "latency_p95_ms", "ms"),
            MetricDefinition("utif_exec_latency", "p(99)", "latency_p99_ms", "ms"),
            MetricDefinition("utif_exec_errors", "rate", "error_rate_ratio", "ratio"),
        ],
        budgets=[
            MetricBudget("latency_p95_ms", "<=", 15.0, "ms"),
            MetricBudget("latency_p99_ms", "<=", 25.0, "ms"),
            MetricBudget("error_rate_ratio", "<", 0.005, "ratio"),
        ],
        trials=trials,
        warmup_seconds=warmup_seconds,
        summary_prefix="utif_exec",
        extra_env=extra_env,
    )


def build_policy_spec(
    trials: int,
    warmup_seconds: int,
    *,
    env: dict | None = None,
) -> ProbeSpec:
    env = env or os.environ
    script = ProbeScript(ROOT / "scripts/perf/policy_perf.js")
    duration_default = env.get("SHELLDONE_PERF_DURATION", "30s")
    extra_env = {
        "SHELLDONE_PERF_DURATION": env.get("SHELLDONE_PERF_POLICY_DURATION", duration_default),
        "SHELLDONE_PERF_RATE": env.get("SHELLDONE_PERF_POLICY_RATE", env.get("SHELLDONE_PERF_RATE", "100")),
        "SHELLDONE_PERF_VUS": env.get("SHELLDONE_PERF_POLICY_VUS", "30"),
        "SHELLDONE_PERF_MAX_VUS": env.get("SHELLDONE_PERF_POLICY_MAX_VUS", "60"),
        "SHELLDONE_PERF_WARMUP_SEC": env.get("SHELLDONE_PERF_POLICY_WARMUP_SEC", env.get("SHELLDONE_PERF_WARMUP_SEC", "0")),
    }
    return ProbeSpec(
        probe_id="policy_perf",
        label="Policy enforcement mix",
        script=script,
        metrics=[
            MetricDefinition("policy_allowed_latency", "p(95)", "allowed_latency_p95_ms", "ms"),
            MetricDefinition("policy_allowed_latency", "p(99)", "allowed_latency_p99_ms", "ms"),
            MetricDefinition("policy_denied_latency", "p(95)", "denied_latency_p95_ms", "ms"),
            MetricDefinition("policy_errors", "rate", "policy_error_rate_ratio", "ratio"),
        ],
        budgets=[
            MetricBudget("allowed_latency_p95_ms", "<=", 20.0, "ms"),
            MetricBudget("allowed_latency_p99_ms", "<=", 30.0, "ms"),
            MetricBudget("denied_latency_p95_ms", "<=", 10.0, "ms"),
            MetricBudget("policy_error_rate_ratio", "<", 0.01, "ratio"),
        ],
        trials=trials,
        warmup_seconds=warmup_seconds,
        summary_prefix="policy_perf",
        extra_env=extra_env,
    )


def build_experience_hub_spec(
    trials: int,
    warmup_seconds: int,
    *,
    env: dict | None = None,
) -> ProbeSpec:
    env = env or os.environ
    script = ProbeScript(ROOT / "scripts/perf/experience_hub.js")
    duration_default = env.get("SHELLDONE_PERF_DURATION", "30s")
    extra_env = {
        "SHELLDONE_PERF_DURATION": env.get(
            "SHELLDONE_PERF_EXPERIENCE_DURATION", duration_default
        ),
        "SHELLDONE_PERF_RATE": env.get(
            "SHELLDONE_PERF_EXPERIENCE_RATE", env.get("SHELLDONE_PERF_RATE", "80")
        ),
        "SHELLDONE_PERF_VUS": env.get(
            "SHELLDONE_PERF_EXPERIENCE_VUS", env.get("SHELLDONE_PERF_VUS", "30")
        ),
        "SHELLDONE_PERF_MAX_VUS": env.get(
            "SHELLDONE_PERF_EXPERIENCE_MAX_VUS", env.get("SHELLDONE_PERF_MAX_VUS", "60")
        ),
        "SHELLDONE_PERF_WARMUP_SEC": env.get(
            "SHELLDONE_PERF_EXPERIENCE_WARMUP_SEC",
            env.get("SHELLDONE_PERF_WARMUP_SEC", "0"),
        ),
    }
    return ProbeSpec(
        probe_id="experience_hub",
        label="Experience Hub telemetry",
        script=script,
        metrics=[
            MetricDefinition(
                "experience_hub_telemetry_latency", "p(95)", "telemetry_latency_p95_ms", "ms"
            ),
            MetricDefinition(
                "experience_hub_approvals_latency", "p(95)", "approvals_latency_p95_ms", "ms"
            ),
            MetricDefinition(
                "experience_hub_errors", "value", "experience_error_rate_ratio", "ratio"
            ),
        ],
        budgets=[
            MetricBudget("telemetry_latency_p95_ms", "<=", 40.0, "ms"),
            MetricBudget("approvals_latency_p95_ms", "<=", 30.0, "ms"),
            MetricBudget("experience_error_rate_ratio", "<", 0.01, "ratio"),
        ],
        trials=trials,
        warmup_seconds=warmup_seconds,
        summary_prefix="experience_hub",
        extra_env=extra_env,
    )


def build_termbridge_discovery_spec(
    trials: int,
    warmup_seconds: int,
    *,
    env: dict | None = None,
) -> ProbeSpec:
    env = env or os.environ
    script = ProbeScript(ROOT / "scripts/perf/termbridge_discovery.js")
    duration_default = env.get("SHELLDONE_PERF_DURATION", "30s")
    extra_env = {
        "SHELLDONE_PERF_DURATION": env.get(
            "SHELLDONE_PERF_TERMBRIDGE_DURATION", duration_default
        ),
        "SHELLDONE_PERF_RATE": env.get(
            "SHELLDONE_PERF_TERMBRIDGE_RATE", env.get("SHELLDONE_PERF_RATE", "80")
        ),
        "SHELLDONE_PERF_VUS": env.get(
            "SHELLDONE_PERF_TERMBRIDGE_VUS", env.get("SHELLDONE_PERF_VUS", "20")
        ),
        "SHELLDONE_PERF_MAX_VUS": env.get(
            "SHELLDONE_PERF_TERMBRIDGE_MAX_VUS", env.get("SHELLDONE_PERF_MAX_VUS", "40")
        ),
        "SHELLDONE_PERF_WARMUP_SEC": env.get(
            "SHELLDONE_PERF_TERMBRIDGE_WARMUP_SEC",
            env.get("SHELLDONE_PERF_WARMUP_SEC", "0"),
        ),
    }
    return ProbeSpec(
        probe_id="termbridge_discovery",
        label="TermBridge discovery registry sync",
        script=script,
        metrics=[
            MetricDefinition(
                "termbridge_discovery_latency", "p(95)", "discovery_latency_p95_ms", "ms"
            ),
            MetricDefinition(
                "termbridge_discovery_latency", "p(99)", "discovery_latency_p99_ms", "ms"
            ),
            MetricDefinition(
                "termbridge_discovery_errors", "rate", "discovery_error_rate_ratio", "ratio"
            ),
        ],
        budgets=[
            MetricBudget("discovery_latency_p95_ms", "<=", 200.0, "ms"),
            MetricBudget("discovery_latency_p99_ms", "<=", 300.0, "ms"),
            MetricBudget("discovery_error_rate_ratio", "<", 0.005, "ratio"),
        ],
        trials=trials,
        warmup_seconds=warmup_seconds,
        summary_prefix="termbridge_discovery",
        extra_env=extra_env,
    )

def build_termbridge_consent_spec(
    trials: int,
    warmup_seconds: int,
    *,
    env: dict | None = None,
) -> ProbeSpec:
    env = env or os.environ
    script = ProbeScript(ROOT / "scripts/perf/termbridge_consent.js")
    duration_default = env.get("SHELLDONE_PERF_DURATION", "30s")
    extra_env = {
        "SHELLDONE_PERF_DURATION": env.get(
            "SHELLDONE_PERF_CONSENT_DURATION", duration_default
        ),
        "SHELLDONE_PERF_RATE": env.get(
            "SHELLDONE_PERF_CONSENT_RATE", env.get("SHELLDONE_PERF_RATE", "80")
        ),
        "SHELLDONE_PERF_VUS": env.get(
            "SHELLDONE_PERF_CONSENT_VUS", env.get("SHELLDONE_PERF_VUS", "20")
        ),
        "SHELLDONE_PERF_MAX_VUS": env.get(
            "SHELLDONE_PERF_CONSENT_MAX_VUS", env.get("SHELLDONE_PERF_MAX_VUS", "40")
        ),
        "SHELLDONE_PERF_WARMUP_SEC": env.get(
            "SHELLDONE_PERF_CONSENT_WARMUP_SEC",
            env.get("SHELLDONE_PERF_WARMUP_SEC", "0"),
        ),
        # Optional: force a specific terminal id
        "SHELLDONE_PERF_CONSENT_TERMINAL": env.get(
            "SHELLDONE_PERF_CONSENT_TERMINAL", ""
        ),
    }
    return ProbeSpec(
        probe_id="termbridge_consent",
        label="TermBridge consent grant/revoke",
        script=script,
        metrics=[
            MetricDefinition(
                "termbridge_consent_latency", "p(95)", "consent_latency_p95_ms", "ms"
            ),
            MetricDefinition(
                "termbridge_consent_latency", "p(99)", "consent_latency_p99_ms", "ms"
            ),
            MetricDefinition(
                "termbridge_consent_errors", "rate", "consent_error_rate_ratio", "ratio"
            ),
        ],
        budgets=[
            MetricBudget("consent_latency_p95_ms", "<=", 50.0, "ms"),
            MetricBudget("consent_latency_p99_ms", "<=", 100.0, "ms"),
            MetricBudget("consent_error_rate_ratio", "<", 0.005, "ratio"),
        ],
        trials=trials,
        warmup_seconds=warmup_seconds,
        summary_prefix="termbridge_consent",
        extra_env=extra_env,
    )


def default_specs(
    trials: int,
    warmup_seconds: int,
    policy_warmup_seconds: int,
    *,
    env: dict | None = None,
) -> List[ProbeSpec]:
    return [
        build_utif_exec_spec(trials, warmup_seconds, env=env),
        build_termbridge_discovery_spec(trials, warmup_seconds, env=env),
        build_termbridge_consent_spec(trials, warmup_seconds, env=env),
        build_policy_spec(trials, policy_warmup_seconds, env=env),
        build_experience_hub_spec(trials, warmup_seconds, env=env),
    ]

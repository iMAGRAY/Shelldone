from __future__ import annotations

from dataclasses import dataclass
from typing import Mapping, Optional


@dataclass(frozen=True)
class ProfileConfig:
    name: str
    trials: int
    warmup_sec: int
    policy_warmup_sec: int
    env_overrides: dict[str, str]


_PROFILES: dict[str, ProfileConfig] = {
    "dev": ProfileConfig(
        name="dev",
        trials=1,
        warmup_sec=5,
        policy_warmup_sec=5,
        env_overrides={
            "SHELLDONE_PERF_DURATION": "20s",
            "SHELLDONE_PERF_POLICY_DURATION": "20s",
            "SHELLDONE_PERF_RATE": "120",
            "SHELLDONE_PERF_POLICY_RATE": "80",
            "SHELLDONE_PERF_WARMUP_SEC": "5",
            "SHELLDONE_PERF_POLICY_WARMUP_SEC": "5",
        },
    ),
    "ci": ProfileConfig(
        name="ci",
        trials=1,
        warmup_sec=5,
        policy_warmup_sec=5,
        env_overrides={
            "SHELLDONE_PERF_DURATION": "30s",
            "SHELLDONE_PERF_POLICY_DURATION": "30s",
            "SHELLDONE_PERF_RATE": "180",
            "SHELLDONE_PERF_POLICY_RATE": "100",
            "SHELLDONE_PERF_WARMUP_SEC": "5",
            "SHELLDONE_PERF_POLICY_WARMUP_SEC": "5",
        },
    ),
    "full": ProfileConfig(
        name="full",
        trials=3,
        warmup_sec=10,
        policy_warmup_sec=10,
        env_overrides={
            "SHELLDONE_PERF_DURATION": "60s",
            "SHELLDONE_PERF_POLICY_DURATION": "60s",
            "SHELLDONE_PERF_RATE": "200",
            "SHELLDONE_PERF_POLICY_RATE": "100",
            "SHELLDONE_PERF_WARMUP_SEC": "10",
            "SHELLDONE_PERF_POLICY_WARMUP_SEC": "10",
        },
    ),
    "staging": ProfileConfig(
        name="staging",
        trials=2,
        warmup_sec=10,
        policy_warmup_sec=10,
        env_overrides={
            "SHELLDONE_PERF_DURATION": "45s",
            "SHELLDONE_PERF_POLICY_DURATION": "45s",
            "SHELLDONE_PERF_RATE": "200",
            "SHELLDONE_PERF_POLICY_RATE": "110",
            "SHELLDONE_PERF_WARMUP_SEC": "10",
            "SHELLDONE_PERF_POLICY_WARMUP_SEC": "10",
        },
    ),
    "prod": ProfileConfig(
        name="prod",
        trials=5,
        warmup_sec=15,
        policy_warmup_sec=15,
        env_overrides={
            "SHELLDONE_PERF_DURATION": "120s",
            "SHELLDONE_PERF_POLICY_DURATION": "120s",
            "SHELLDONE_PERF_RATE": "240",
            "SHELLDONE_PERF_POLICY_RATE": "150",
            "SHELLDONE_PERF_WARMUP_SEC": "15",
            "SHELLDONE_PERF_POLICY_WARMUP_SEC": "15",
        },
    ),
}


def get_profile(name: Optional[str]) -> Optional[ProfileConfig]:
    if name is None:
        return None
    key = name.lower()
    return _PROFILES.get(key)


def apply_env_overrides(
    base_env: Mapping[str, str],
    overrides: Mapping[str, str],
    *,
    overwrite: bool = False,
) -> dict[str, str]:
    result = dict(base_env)
    for key, value in overrides.items():
        if overwrite or key not in result:
            result[key] = value
    return result

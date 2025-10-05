from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Iterable


@dataclass(frozen=True)
class MetricDefinition:
    """Defines how to extract a single metric from a probe run."""

    metric_id: str
    statistic: str
    alias: str
    unit: str

    def __post_init__(self) -> None:
        for field_name, value in (
            ("metric_id", self.metric_id.strip()),
            ("statistic", self.statistic.strip()),
            ("alias", self.alias.strip()),
            ("unit", self.unit.strip()),
        ):
            if not value:
                raise ValueError(f"{field_name} cannot be empty")
        object.__setattr__(self, "metric_id", self.metric_id.strip())
        object.__setattr__(self, "statistic", self.statistic.strip())
        object.__setattr__(self, "alias", self.alias.strip())
        object.__setattr__(self, "unit", self.unit.strip())


@dataclass(frozen=True)
class MetricBudget:
    """Budget guardrail for a metric alias."""

    alias: str
    comparator: str
    limit: float
    unit: str

    def __post_init__(self) -> None:
        alias = self.alias.strip()
        if not alias:
            raise ValueError("alias cannot be empty")
        comparator = self.comparator.strip()
        if comparator not in {"<=", "<"}:
            raise ValueError("comparator must be '<=' or '<'")
        if self.limit < 0:
            raise ValueError("limit must be non-negative")
        unit = self.unit.strip()
        if not unit:
            raise ValueError("unit cannot be empty")
        object.__setattr__(self, "alias", alias)
        object.__setattr__(self, "comparator", comparator)
        object.__setattr__(self, "unit", unit)

    def is_satisfied(self, value: float) -> bool:
        if self.comparator == "<=":
            return value <= self.limit
        return value < self.limit


@dataclass(frozen=True)
class MetricValue:
    """Measured value for a metric alias."""

    alias: str
    value: float
    unit: str

    def __post_init__(self) -> None:
        alias = self.alias.strip()
        unit = self.unit.strip()
        if not alias:
            raise ValueError("alias cannot be empty")
        if not unit:
            raise ValueError("unit cannot be empty")
        object.__setattr__(self, "alias", alias)
        object.__setattr__(self, "unit", unit)


@dataclass(frozen=True)
class ProbeScript:
    """Represents an executable probe script path."""

    path: Path

    def __post_init__(self) -> None:
        if not isinstance(self.path, Path):
            raise TypeError("path must be a pathlib.Path instance")
        if not self.path.exists():
            raise FileNotFoundError(f"probe script does not exist: {self.path}")
        if not self.path.is_file():
            raise ValueError(f"probe script must be a file: {self.path}")


def ensure_metric_aliases(
    metrics: Iterable[MetricDefinition],
    budgets: Iterable[MetricBudget],
) -> None:
    """Ensures each budget references an existing metric alias."""

    metric_aliases = {metric.alias for metric in metrics}
    missing = [budget.alias for budget in budgets if budget.alias not in metric_aliases]
    if missing:
        joined = ", ".join(sorted(set(missing)))
        raise ValueError(f"budgets reference unknown metric aliases: {joined}")

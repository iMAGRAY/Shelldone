from __future__ import annotations

from dataclasses import dataclass, field
from statistics import median
from typing import Dict, Iterable, List

from .value_objects import (
    MetricBudget,
    MetricDefinition,
    MetricValue,
    ProbeScript,
    ensure_metric_aliases,
)


@dataclass(frozen=True)
class BudgetStatus:
    alias: str
    comparator: str
    limit: float
    unit: str
    actual: float
    passed: bool


@dataclass(frozen=True)
class ProbeTrialResult:
    trial_index: int
    metrics: Dict[str, MetricValue]
    summary_path: str

    def __post_init__(self) -> None:
        if self.trial_index < 0:
            raise ValueError("trial_index must be non-negative")
        if not self.metrics:
            raise ValueError("metrics cannot be empty")
        for alias, metric in self.metrics.items():
            if alias != metric.alias:
                raise ValueError("metric alias mismatch in trial result")


@dataclass(frozen=True)
class ProbeSpec:
    probe_id: str
    label: str
    script: ProbeScript
    metrics: List[MetricDefinition]
    budgets: List[MetricBudget]
    trials: int = 3
    warmup_seconds: int = 10
    summary_prefix: str = ""
    extra_env: Dict[str, str] = field(default_factory=dict)

    def __post_init__(self) -> None:
        probe_id = self.probe_id.strip()
        label = self.label.strip()
        if not probe_id:
            raise ValueError("probe_id cannot be empty")
        if not label:
            raise ValueError("label cannot be empty")
        if not self.metrics:
            raise ValueError("probe must declare at least one metric")
        if self.trials < 1:
            raise ValueError("trials must be >= 1")
        if self.warmup_seconds < 0:
            raise ValueError("warmup_seconds cannot be negative")
        ensure_metric_aliases(self.metrics, self.budgets)
        object.__setattr__(self, "probe_id", probe_id)
        object.__setattr__(self, "label", label)
        if not self.summary_prefix:
            object.__setattr__(self, "summary_prefix", probe_id)


@dataclass
class ProbeAggregate:
    spec: ProbeSpec
    trials: List[ProbeTrialResult] = field(default_factory=list)

    def add_trial(self, result: ProbeTrialResult) -> None:
        if result.trial_index >= self.spec.trials:
            raise ValueError("trial index exceeds configured count")
        expected_aliases = {metric.alias for metric in self.spec.metrics}
        if set(result.metrics.keys()) != expected_aliases:
            raise ValueError("trial metrics mismatch specification")
        self.trials.append(result)

    def aggregated_metrics(self) -> Dict[str, MetricValue]:
        if not self.trials:
            raise ValueError("no trials recorded for aggregation")
        aggregated: Dict[str, MetricValue] = {}
        for metric in self.spec.metrics:
            series = [trial.metrics[metric.alias].value for trial in self.trials]
            aggregated[metric.alias] = MetricValue(
                alias=metric.alias,
                value=median(series),
                unit=metric.unit,
            )
        return aggregated

    def evaluate_budgets(self) -> List[BudgetStatus]:
        values = self.aggregated_metrics()
        evaluations: List[BudgetStatus] = []
        for budget in self.spec.budgets:
            actual = values[budget.alias].value
            passed = budget.is_satisfied(actual)
            evaluations.append(
                BudgetStatus(
                    alias=budget.alias,
                    comparator=budget.comparator,
                    limit=budget.limit,
                    unit=budget.unit,
                    actual=actual,
                    passed=passed,
                )
            )
        return evaluations

    def to_report(self) -> ProbeReport:
        return ProbeReport(
            probe_id=self.spec.probe_id,
            label=self.spec.label,
            aggregated=self.aggregated_metrics(),
            budgets=self.evaluate_budgets(),
            trials=list(self.trials),
        )


@dataclass(frozen=True)
class ProbeReport:
    probe_id: str
    label: str
    aggregated: Dict[str, MetricValue]
    budgets: List[BudgetStatus]
    trials: List[ProbeTrialResult]

    def has_failures(self) -> bool:
        return any(not budget.passed for budget in self.budgets)

    def violated_budgets(self) -> Iterable[BudgetStatus]:
        return (budget for budget in self.budgets if not budget.passed)

    def to_dict(self) -> dict:
        return {
            "probe_id": self.probe_id,
            "label": self.label,
            "aggregated": {
                alias: {"value": metric.value, "unit": metric.unit}
                for alias, metric in self.aggregated.items()
            },
            "budgets": [
                {
                    "alias": budget.alias,
                    "comparator": budget.comparator,
                    "limit": budget.limit,
                    "unit": budget.unit,
                    "actual": budget.actual,
                    "passed": budget.passed,
                }
                for budget in self.budgets
            ],
            "trials": [
                {
                    "trial_index": trial.trial_index,
                    "summary_path": trial.summary_path,
                    "metrics": {
                        alias: {"value": metric.value, "unit": metric.unit}
                        for alias, metric in trial.metrics.items()
                    },
                }
                for trial in self.trials
            ],
        }


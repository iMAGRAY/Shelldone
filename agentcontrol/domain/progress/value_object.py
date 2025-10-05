"""Value objects for the progress bounded context."""
from __future__ import annotations

from dataclasses import dataclass
from typing import Mapping


STATUS_WEIGHTS: Mapping[str, float] = {
    "done": 1.0,
    "review": 0.9,
    "ready": 0.75,
    "in_progress": 0.5,
    "at_risk": 0.4,
    "blocked": 0.3,
    "planned": 0.0,
    "backlog": 0.0,
}


@dataclass(frozen=True, slots=True)
class ProgressValue:
    """Immutable percentage value clamped between 0 and 100."""

    value: int

    def __post_init__(self) -> None:
        if not isinstance(self.value, int):  # pragma: no cover - defensive guard
            raise TypeError("ProgressValue must wrap an int")
        if self.value < 0 or self.value > 100:
            raise ValueError(f"ProgressValue {self.value} is outside [0, 100]")

    @classmethod
    def from_ratio(cls, ratio: float) -> "ProgressValue":
        numeric = max(0.0, min(1.0, float(ratio)))
        return cls(int(round(numeric * 100)))

    @classmethod
    def from_status(cls, status: str, weights: Mapping[str, float] | None = None) -> "ProgressValue":
        table = weights or STATUS_WEIGHTS
        if status not in table:
            raise ValueError(f"Unknown status '{status}'")
        return cls.from_ratio(table[status])

    def adjusted(self, fallback: int) -> "ProgressValue":
        if fallback < 0:
            return ProgressValue(0)
        if fallback > 100:
            return ProgressValue(100)
        return ProgressValue(fallback)

    @property
    def status(self) -> str:
        if self.value >= 100:
            return "done"
        if self.value <= 0:
            return "planned"
        return "in_progress"

    def delta(self, other: "ProgressValue") -> int:
        return self.value - other.value

    def __int__(self) -> int:
        return self.value

    def __repr__(self) -> str:  # pragma: no cover - convenience only
        return f"ProgressValue({self.value})"


@dataclass(frozen=True, slots=True)
class Weight:
    """Positive numeric weight used for aggregation."""

    value: float

    def __post_init__(self) -> None:
        numeric = float(self.value)
        if numeric <= 0:
            raise ValueError("Weight must be positive")
        object.__setattr__(self, "value", numeric)

    def __float__(self) -> float:
        return self.value

    def __repr__(self) -> str:  # pragma: no cover - convenience only
        return f"Weight({self.value})"

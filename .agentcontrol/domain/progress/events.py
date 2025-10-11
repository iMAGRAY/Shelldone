"""Domain events emitted by the progress aggregate."""
from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime, timezone
from typing import List

from .value_object import ProgressValue


@dataclass(slots=True)
class ProgressRecomputed:
    """Signal that the program progress snapshot changed."""

    program_id: str
    computed_progress: ProgressValue
    manual_progress: ProgressValue
    warnings: List[str]
    occurred_at: datetime

    @classmethod
    def emit(cls, program_id: str, computed: ProgressValue, manual: ProgressValue, warnings: List[str]) -> "ProgressRecomputed":
        return cls(
            program_id=program_id,
            computed_progress=computed,
            manual_progress=manual,
            warnings=list(warnings),
            occurred_at=datetime.now(timezone.utc),
        )

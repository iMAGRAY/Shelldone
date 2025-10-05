from __future__ import annotations

from abc import ABC, abstractmethod
from pathlib import Path

from ..domain.probe import ProbeSpec, ProbeTrialResult


class ProbeExecutionError(RuntimeError):
    """Raised when executing a probe fails."""


class ProbeRunnerPort(ABC):
    """Port for executing performance probes."""

    @abstractmethod
    def run(
        self,
        spec: ProbeSpec,
        trial_index: int,
        output_dir: Path,
    ) -> ProbeTrialResult:
        """Executes a single trial for the provided spec and returns structured metrics."""

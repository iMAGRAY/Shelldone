"""Aggregate representing the Shelldone program progress state."""
from __future__ import annotations

from collections import defaultdict, OrderedDict
from dataclasses import dataclass, field
from datetime import datetime, timezone
from typing import Dict, Iterable, List, Mapping, Optional, Sequence, Tuple

from .events import ProgressRecomputed
from .value_object import ProgressValue, STATUS_WEIGHTS, Weight


def _coerce_progress(value: object) -> int:
    try:
        return int(round(float(value)))
    except (TypeError, ValueError):
        return 0


def _extract_manual_progress(definition: Mapping[str, object], fallback_status: str) -> ProgressValue:
    metrics_block = definition.get("metrics", {}) if isinstance(definition.get("metrics", {}), Mapping) else {}
    manual_raw = metrics_block.get("progress_pct") if metrics_block else None
    if manual_raw is None:
        manual_raw = definition.get("progress_pct")
    manual_int = _coerce_progress(manual_raw)
    if manual_int <= 0:
        return ProgressValue.from_status(str(definition.get("status", fallback_status)))
    return ProgressValue(min(100, max(0, manual_int)))


@dataclass(slots=True)
class BigTaskProgressState:
    """Computed projection for a single big task."""

    big_task_id: str
    title: str
    parent_epic: str
    size_points: Weight
    computed: ProgressValue
    manual: ProgressValue
    status: str
    roadmap_phase: Optional[str]
    warnings: List[str] = field(default_factory=list)

    @classmethod
    def from_sources(
        cls,
        definition: Mapping[str, object],
        board_tasks: Sequence[Mapping[str, object]],
        fallback_status: str,
    ) -> "BigTaskProgressState":
        big_task_id = str(definition["id"])
        title = str(definition.get("title", big_task_id))
        parent_epic = str(definition.get("parent_epic", ""))
        raw_points = definition.get("size_points", 1) or 1
        size = Weight(float(raw_points))
        roadmap_phase = definition.get("roadmap_phase")

        manual_value = _extract_manual_progress(definition, fallback_status)

        total_weight = 0.0
        accumulated = 0.0
        for task in board_tasks:
            status = str(task.get("status", "backlog"))
            weight = float(task.get("size_points", raw_points) or raw_points)
            total_weight += max(weight, 0.1)
            accumulated += STATUS_WEIGHTS.get(status, 0.0) * max(weight, 0.1)

        warnings: List[str] = []
        if total_weight:
            computed = ProgressValue.from_ratio(accumulated / total_weight)
        else:
            computed = manual_value
            warnings.append(f"Big Task {big_task_id} lacks board coverage; using manual progress {manual_value.value}%")

        status_value = computed.status
        delta = computed.delta(manual_value)
        if abs(delta) > 5:
            warnings.append(
                f"Big Task {big_task_id} manual {manual_value.value}% differs from computed {computed.value}% by {delta:+d}"
            )

        return cls(
            big_task_id=big_task_id,
            title=title,
            parent_epic=parent_epic,
            size_points=size,
            computed=computed,
            manual=manual_value,
            status=status_value,
            roadmap_phase=str(roadmap_phase) if roadmap_phase else None,
            warnings=warnings,
        )


@dataclass(slots=True)
class EpicProgressState:
    """Computed projection for an epic."""

    epic_id: str
    title: str
    size_points: Weight
    computed: ProgressValue
    manual: ProgressValue
    status: str
    warnings: List[str] = field(default_factory=list)

    @classmethod
    def from_sources(
        cls,
        definition: Mapping[str, object],
        big_tasks: Sequence[BigTaskProgressState],
        board_tasks: Sequence[Mapping[str, object]],
    ) -> "EpicProgressState":
        epic_id = str(definition["id"])
        title = str(definition.get("title", epic_id))
        raw_points = definition.get("size_points", 1) or 1
        size = Weight(float(raw_points))

        manual_value = _extract_manual_progress(definition, str(definition.get("status", "planned")))

        relevant_big = [bt for bt in big_tasks if bt.parent_epic == epic_id]
        total_weight = sum(float(bt.size_points) for bt in relevant_big)
        accumulated = sum(float(bt.size_points) * (bt.computed.value / 100) for bt in relevant_big)

        residual_tasks = [task for task in board_tasks if task.get("epic") == epic_id and not task.get("big_task")]
        residual_weight = 0.0
        residual_acc = 0.0
        for task in residual_tasks:
            status = str(task.get("status", "backlog"))
            weight = float(task.get("size_points", raw_points) or raw_points)
            residual_weight += max(weight, 0.1)
            residual_acc += STATUS_WEIGHTS.get(status, 0.0) * max(weight, 0.1)

        total_weight += residual_weight
        accumulated += residual_acc

        warnings: List[str] = []
        if total_weight:
            computed = ProgressValue.from_ratio(accumulated / total_weight)
        else:
            computed = manual_value
            warnings.append(f"Epic {epic_id} lacks weighted data; using manual progress {manual_value.value}%")

        delta = computed.delta(manual_value)
        if abs(delta) > 5:
            warnings.append(
                f"Epic {epic_id} manual {manual_value.value}% differs from computed {computed.value}% by {delta:+d}"
            )

        status_value = computed.status
        return cls(
            epic_id=epic_id,
            title=title,
            size_points=size,
            computed=computed,
            manual=manual_value,
            status=status_value,
            warnings=warnings,
        )


@dataclass(slots=True)
class ProgramProgressAggregate:
    """Aggregate representing the full program progress snapshot."""

    program_id: str
    name: str
    computed_progress: ProgressValue
    manual_progress: ProgressValue
    health: str
    phase_progress: Mapping[str, ProgressValue]
    milestones: List[Mapping[str, object]]
    epics: List[EpicProgressState]
    big_tasks: List[BigTaskProgressState]
    warnings: List[str]
    events: List[ProgressRecomputed]
    generated_at: datetime

    @classmethod
    def from_sources(
        cls,
        manifest: Mapping[str, object],
        task_board: Mapping[str, object],
    ) -> "ProgramProgressAggregate":
        program = manifest.get("program", {})
        meta = program.get("meta", {})
        progress_block = program.get("progress", {})
        program_id = str(meta.get("program_id", "program"))
        name = str(meta.get("name", "Program"))
        manual_progress = ProgressValue(min(100, max(0, _coerce_progress(progress_block.get("progress_pct")))))
        health = str(progress_block.get("health", "yellow"))

        board_tasks = task_board.get("tasks", [])
        manifest_big_tasks = manifest.get("big_tasks", [])
        manifest_epics = manifest.get("epics", [])
        manifest_tasks = manifest.get("tasks", [])

        # Map board tasks per big task once for reuse
        board_by_big: Dict[str, List[Mapping[str, object]]] = defaultdict(list)
        for task in board_tasks:
            bt = task.get("big_task")
            if bt:
                board_by_big[str(bt)].append(task)

        big_states: List[BigTaskProgressState] = []
        warnings: List[str] = []
        for big in manifest_big_tasks:
            state = BigTaskProgressState.from_sources(big, board_by_big.get(str(big["id"]), []), str(big.get("status", "planned")))
            big_states.append(state)
            warnings.extend(state.warnings)

        # Identify board big tasks missing in manifest
        known_big_ids = {bt.big_task_id for bt in big_states}
        stray_big = {str(task.get("big_task")) for task in board_tasks if task.get("big_task") and str(task.get("big_task")) not in known_big_ids}
        for missing_id in sorted(stray_big):
            warnings.append(f"Task board references unknown Big Task '{missing_id}'")

        epic_states: List[EpicProgressState] = []
        board_by_epic: Dict[str, List[Mapping[str, object]]] = defaultdict(list)
        for task in board_tasks:
            epic_id = task.get("epic")
            if epic_id:
                board_by_epic[str(epic_id)].append(task)

        for epic in manifest_epics:
            state = EpicProgressState.from_sources(epic, big_states, board_by_epic.get(str(epic["id"]), []))
            epic_states.append(state)
            warnings.extend(state.warnings)

        total_weight = sum(float(epic.size_points) for epic in epic_states)
        accumulated = sum(float(epic.size_points) * (epic.computed.value / 100) for epic in epic_states)
        if total_weight:
            computed_program = ProgressValue.from_ratio(accumulated / total_weight)
        else:
            computed_program = manual_progress
            warnings.append("Program lacks epic weighting; using manual progress")

        delta = computed_program.delta(manual_progress)
        if abs(delta) > 5:
            warnings.append(
                f"Program manual progress {manual_progress.value}% differs from computed {computed_program.value}% by {delta:+d}"
            )

        # Phase aggregation
        milestones = list(program.get("milestones", []))
        phase_acc_weight: Dict[str, float] = defaultdict(float)
        phase_acc_value: Dict[str, float] = defaultdict(float)
        for state in big_states:
            if not state.roadmap_phase:
                continue
            weight = float(state.size_points)
            phase_acc_weight[state.roadmap_phase] += weight
            phase_acc_value[state.roadmap_phase] += weight * (state.computed.value / 100)

        # include manifest tasks coverage for phases without big task mapping
        for task in manifest_tasks:
            phase = task.get("roadmap_phase")
            if not phase:
                continue
            status = str(task.get("status", "planned"))
            weight = float(task.get("size_points", 1) or 1)
            phase_acc_weight[str(phase)] += weight
            phase_acc_value[str(phase)] += weight * STATUS_WEIGHTS.get(status, 0.0)

        phase_progress: "OrderedDict[str, ProgressValue]" = OrderedDict()
        milestone_order = [m.get("title") for m in milestones if m.get("title")]
        phase_lookup = {m.get("id"): m for m in milestones if m.get("id")}

        def resolve_phase_value(phase_id: str) -> ProgressValue:
            weight = phase_acc_weight.get(phase_id, 0.0)
            if weight:
                return ProgressValue.from_ratio(phase_acc_value[phase_id] / weight)
            return computed_program

        for milestone in milestones:
            phase_id = milestone.get("id")
            title = milestone.get("title")
            if not phase_id or not title:
                continue
            value = resolve_phase_value(phase_id)
            phase_progress[title] = value

        # ensure deterministic order even if milestones absent
        for phase_id, weight in phase_acc_weight.items():
            if phase_id not in phase_lookup:
                title = str(phase_id)
                if title not in phase_progress:
                    calculated = ProgressValue.from_ratio(phase_acc_value[phase_id] / weight)
                    phase_progress[title] = calculated

        events = [ProgressRecomputed.emit(program_id, computed_program, manual_progress, warnings)]
        generated_at = datetime.now(timezone.utc)
        return cls(
            program_id=program_id,
            name=name,
            computed_progress=computed_program,
            manual_progress=manual_progress,
            health=health,
            phase_progress=phase_progress,
            milestones=milestones,
            epics=epic_states,
            big_tasks=big_states,
            warnings=warnings,
            events=events,
            generated_at=generated_at,
        )

    def to_serialisable(self) -> Dict[str, object]:
        return {
            "program": {
                "id": self.program_id,
                "name": self.name,
                "progress_pct": self.computed_progress.value,
                "manual_progress_pct": self.manual_progress.value,
                "health": self.health,
                "phase_progress": {title: value.value for title, value in self.phase_progress.items()},
                "milestones": self.milestones,
            },
            "epics": [
                {
                    "id": epic.epic_id,
                    "title": epic.title,
                    "size_points": float(epic.size_points),
                    "status": epic.status,
                    "computed_progress_pct": epic.computed.value,
                    "manual_progress_pct": epic.manual.value,
                }
                for epic in self.epics
            ],
            "big_tasks": [
                {
                    "id": bt.big_task_id,
                    "title": bt.title,
                    "parent_epic": bt.parent_epic,
                    "size_points": float(bt.size_points),
                    "status": bt.status,
                    "computed_progress_pct": bt.computed.value,
                    "manual_progress_pct": bt.manual.value,
                    "roadmap_phase": bt.roadmap_phase,
                }
                for bt in self.big_tasks
            ],
            "warnings": list(self.warnings),
            "generated_at": self.generated_at.isoformat(),
        }

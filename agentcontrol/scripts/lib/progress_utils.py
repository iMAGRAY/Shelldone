"""Утилиты пересчёта прогресса для SDK."""
from __future__ import annotations

from datetime import datetime, timezone
from typing import Dict, Iterable, Mapping

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

PHASE_ORDER: tuple[str, ...] = (
    "Phase 0 – Feasibility",
    "Phase 1 – Foundation",
    "Phase 2 – Core Build",
    "Phase 3 – Beta",
    "Phase 4 – GA",
    "Phase 5 – Ops & Scaling",
    "Phase 6 – Optimization",
    "Phase 7 – Sustain & Innovate",
)


def status_score(status: str) -> float:
    try:
        return STATUS_WEIGHTS[status]
    except KeyError as exc:
        raise ValueError(f"Неизвестный статус '{status}'") from exc


def _normalise_weight(weight: float | int | None) -> float:
    if weight is None:
        return 1.0
    try:
        numeric = float(weight)
    except (TypeError, ValueError) as exc:
        raise ValueError(f"Вес '{weight}' нечисловой") from exc
    if numeric <= 0:
        return 1.0
    return numeric


def weighted_status_average(items: Iterable[dict], status_key: str, weight_key: str | None = None) -> int:
    total_weight = 0.0
    accumulated = 0.0
    for item in items:
        status = item.get(status_key)
        if status is None:
            continue
        weight = _normalise_weight(item.get(weight_key)) if weight_key else 1.0
        total_weight += weight
        accumulated += status_score(status) * weight
    if total_weight == 0:
        return 0
    return int(round(accumulated / total_weight * 100))


def weighted_numeric_average(items: Iterable[dict], value_key: str, weight_key: str | None = None) -> int:
    total_weight = 0.0
    accumulated = 0.0
    for item in items:
        value = item.get(value_key)
        if value is None:
            continue
        weight = _normalise_weight(item.get(weight_key)) if weight_key else 1.0
        total_weight += weight
        accumulated += float(value) * weight
    if total_weight == 0:
        return 0
    return int(round(accumulated / total_weight))


def compute_phase_progress(tasks: list[dict], milestones: list[dict], default_value: int) -> Dict[str, int]:
    if not milestones:
        # fallback to legacy phase ordering
        id_to_title = {title: title for title in PHASE_ORDER}
        milestones = [
            {"id": title, "title": title}
            for title in PHASE_ORDER
        ]
    phase_values: Dict[str, int] = {}

    title_lookup = {m.get("id"): m.get("title") for m in milestones if m.get("id") and m.get("title")}
    for milestone in milestones:
        phase_id = milestone.get("id")
        title = milestone.get("title")
        if not phase_id or not title:
            continue
        relevant = [task for task in tasks if task.get("roadmap_phase") == phase_id]
        if relevant:
            phase_values[title] = weighted_status_average(relevant, "status", "size_points")
        else:
            phase_values[title] = default_value

    # preserve legacy order for display when milestones map onto PHASE_ORDER
    ordered: Dict[str, int] = {}
    seen = set()
    for title in PHASE_ORDER:
        if title in phase_values:
            ordered[title] = phase_values[title]
            seen.add(title)
    for title, value in phase_values.items():
        if title not in seen:
            ordered[title] = value
    return ordered


def status_from_progress(progress: int) -> str:
    if progress >= 100:
        return "done"
    if progress <= 0:
        return "planned"
    return "in_progress"


def utc_now_iso() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")

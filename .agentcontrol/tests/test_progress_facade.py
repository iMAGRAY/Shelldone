"""Regression tests for agentcall progress facade."""
from __future__ import annotations

from pathlib import Path

from agentcontrol.scripts import progress


CAPSULE_DIR = Path(__file__).resolve().parents[1]
REPO_ROOT = Path(__file__).resolve().parents[2]
MANIFEST_PATH = CAPSULE_DIR / "architecture" / "manifest.yaml"
TODO_PATH = REPO_ROOT / "todo.machine.md"


def test_load_manifest_is_read_only() -> None:
    before_manifest = MANIFEST_PATH.read_text(encoding="utf-8")
    manifest = progress.load_manifest()
    after_manifest = MANIFEST_PATH.read_text(encoding="utf-8")

    assert manifest["program"]["progress"]["progress_pct"] >= 0
    assert after_manifest == before_manifest


def test_calculate_progress_matches_state_snapshot() -> None:
    manifest_projection = progress.load_manifest()
    manifest_phase_keys = set(
        manifest_projection
        .get("program", {})
        .get("progress", {})
        .get("phase_progress", {})
        .keys()
    )

    program, epics, big_tasks, phase_progress = progress.calculate_progress()

    assert program["computed_pct"] >= 0
    assert isinstance(epics, list) and isinstance(big_tasks, list)
    assert set(phase_progress.keys()) == manifest_phase_keys

    state = progress.collect_progress_state()
    assert state["program"]["progress_pct"] == program["computed_pct"]
    assert state["phase_progress"] == phase_progress


def test_render_progress_tables_produces_ascii_snapshot() -> None:
    program, epics, big_tasks, _ = progress.calculate_progress()
    manifest_projection = progress.load_manifest()

    table = progress.render_progress_tables(program, epics, big_tasks, manifest_projection)

    assert "Программа" in table
    assert "|" in table


def test_collect_progress_state_preserves_todo_text() -> None:
    original_todo = TODO_PATH.read_text(encoding="utf-8")
    state = progress.collect_progress_state()
    assert state["board"]["counts"]["in_progress"] >= 0

    # Ensure no mutation of todo.machine.md happened as a side-effect.
    assert TODO_PATH.read_text(encoding="utf-8") == original_todo

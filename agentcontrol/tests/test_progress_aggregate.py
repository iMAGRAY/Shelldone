import pytest

from agentcontrol.domain.progress.aggregate import ProgramProgressAggregate


@pytest.fixture()
def manifest_fixture():
    return {
        "program": {
            "meta": {"program_id": "test", "name": "Test Program"},
            "progress": {"progress_pct": 10, "health": "yellow"},
            "milestones": [
                {"id": "m1", "title": "Phase One", "due": "2025-12-01", "status": "planned"},
                {"id": "m2", "title": "Phase Two", "due": "2026-01-01", "status": "planned"},
            ],
        },
        "epics": [
            {"id": "epic-a", "title": "Epic A", "status": "in_progress", "size_points": 5},
            {"id": "epic-b", "title": "Epic B", "status": "planned", "size_points": 5},
        ],
        "big_tasks": [
            {
                "id": "bt-a",
                "title": "Big A",
                "parent_epic": "epic-a",
                "status": "in_progress",
                "size_points": 5,
                "roadmap_phase": "m1",
            },
            {
                "id": "bt-b",
                "title": "Big B",
                "parent_epic": "epic-b",
                "status": "planned",
                "size_points": 5,
                "roadmap_phase": "m2",
            },
        ],
    }


@pytest.fixture()
def task_board_fixture():
    return {
        "tasks": [
            {"id": "t1", "status": "in_progress", "size_points": 5, "big_task": "bt-a", "epic": "epic-a"},
            {"id": "t2", "status": "planned", "size_points": 5, "big_task": "bt-b", "epic": "epic-b"},
        ],
    }


def test_aggregate_computes_progress(manifest_fixture, task_board_fixture):
    aggregate = ProgramProgressAggregate.from_sources(manifest_fixture, task_board_fixture)
    assert aggregate.computed_progress.value == 25
    epic_a = next(epic for epic in aggregate.epics if epic.epic_id == "epic-a")
    assert epic_a.computed.value == 50
    phase_values = {title: value.value for title, value in aggregate.phase_progress.items()}
    assert phase_values["Phase One"] == 50
    assert phase_values["Phase Two"] == 0


def test_warnings_emitted_when_board_missing(manifest_fixture):
    board = {"tasks": []}
    aggregate = ProgramProgressAggregate.from_sources(manifest_fixture, board)
    assert any("lacks board coverage" in warning for warning in aggregate.warnings)

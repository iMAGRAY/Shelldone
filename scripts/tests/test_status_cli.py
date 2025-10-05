import json
from pathlib import Path
from unittest import mock

import pytest

import scripts.status as status_cli


@pytest.fixture()
def temporary_status_path(tmp_path: Path):
    with mock.patch.object(status_cli, "STATUS_PATH", tmp_path / "status.json"):
        yield status_cli.STATUS_PATH


def test_load_status_missing(temporary_status_path: Path):
    data = status_cli.load_status()
    assert data == {}


def test_summarise_returns_structured_payload(temporary_status_path: Path):
    payload = {
        "roadmap": {
            "program": {
                "name": "Program",
                "progress_pct": 42,
                "manual_progress_pct": 40,
                "health": "green",
                "phase_progress": {"Phase": 42},
                "milestones": [{"title": "Phase", "due": "2025-12-01", "status": "in_progress"}],
            },
            "warnings": ["delta"],
        },
        "tasks": {"counts": {"done": 2}, "updated_at": "now", "board_version": "0.1.0"},
        "generated_at": "timestamp",
    }
    temporary_status_path.write_text(json.dumps(payload), encoding="utf-8")

    summary = status_cli.summarise(status_cli.load_status())
    assert summary["program"]["progress_pct"] == 42
    assert summary["warnings"] == ["delta"]
    assert summary["tasks"]["counts"]["done"] == 2


def test_format_text_contains_key_sections(temporary_status_path: Path):
    payload = {
        "roadmap": {
            "program": {
                "name": "Test Program",
                "progress_pct": 10,
                "manual_progress_pct": 8,
                "health": "yellow",
                "phase_progress": {"Alpha": 10},
                "milestones": [{"title": "Alpha", "due": "2025-11-01", "status": "in_progress"}],
            }
        },
        "tasks": {"counts": {"planned": 3}},
    }
    temporary_status_path.write_text(json.dumps(payload), encoding="utf-8")
    summary = status_cli.summarise(status_cli.load_status())
    text = status_cli.format_text(summary)
    assert "Test Program" in text
    assert "Alpha" in text
    assert "planned" in text

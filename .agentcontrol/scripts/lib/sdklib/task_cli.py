#!/usr/bin/env python3
"""Высокопроизводительный CLI для управления доской задач SDK.

Переосмысленная реализация task.sh: добавляет блокировки, кэширование,
расширенные метрики и устойчивость для работы с десятками и сотнями агентов.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from collections import Counter, defaultdict, deque
from contextlib import contextmanager
from dataclasses import dataclass, field
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any, Iterable

try:  # POSIX-ориентированная блокировка файлов
    import fcntl  # type: ignore
except ModuleNotFoundError:  # pragma: no cover - ожидаемо на non-POSIX
    fcntl = None  # type: ignore


# --- Путь окружения -----------------------------------------------------

def detect_sdk_root() -> Path:
    env = os.environ.get("SDK_ROOT")
    if env:
        return Path(env).resolve()
    return Path(__file__).resolve().parents[3]


SDK_ROOT = detect_sdk_root()
BOARD_PATH = SDK_ROOT / "data" / "tasks.board.json"
STATE_PATH = SDK_ROOT / "state" / "task_state.json"
LOG_PATH = SDK_ROOT / "journal" / "task_events.jsonl"
LOCK_PATH = SDK_ROOT / "state" / ".sdk.lock"


# --- Константы -----------------------------------------------------------

STATUS_ORDER = [
    "in_progress",
    "review",
    "ready",
    "backlog",
    "blocked",
    "done",
]
STATUS_TITLES = {
    "in_progress": "In Progress",
    "review": "Review",
    "ready": "Ready",
    "backlog": "Backlog",
    "blocked": "Blocked",
    "done": "Done",
}
PRIORITY_RANK = {"P0": 0, "P1": 1, "P2": 2, "P3": 3}
STATUS_PROGRESS = {
    "done": 1.0,
    "review": 0.9,
    "ready": 0.75,
    "in_progress": 0.5,
    "backlog": 0.0,
    "blocked": 0.0,
}
DEFAULT_OWNER = "unassigned"


# --- Вспомогательные структуры -----------------------------------------


def now_iso() -> str:
    return datetime.now(timezone.utc).isoformat()


def parse_time(value: str | None) -> datetime | None:
    if not value:
        return None
    try:
        return datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        return None


def priority_rank(task: dict) -> int:
    return PRIORITY_RANK.get(task.get("priority", "P3"), 99)


def status_rank(task: dict) -> int:
    try:
        return STATUS_ORDER.index(task.get("status", "backlog"))
    except ValueError:
        return 99


@contextmanager
def global_lock(*, exclusive: bool) -> Iterable[None]:
    """Глобальная блокировка для синхронизации сотен агентов."""

    if fcntl is None:  # pragma: no cover - fallback без блокировок
        yield
        return

    LOCK_PATH.parent.mkdir(parents=True, exist_ok=True)
    with open(LOCK_PATH, "w", encoding="utf-8") as lock_file:
        mode = fcntl.LOCK_EX if exclusive else fcntl.LOCK_SH  # type: ignore[attr-defined]
        fcntl.flock(lock_file, mode)  # type: ignore[attr-defined]
        try:
            yield
        finally:
            fcntl.flock(lock_file, fcntl.LOCK_UN)  # type: ignore[attr-defined]


def read_json(path: Path, default: Any) -> Any:
    if not path.exists():
        return default
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except Exception:
        return default


def write_json_atomic(path: Path, data: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + f".{os.getpid()}.tmp")
    tmp.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    tmp.replace(path)


def normalize_task(task: dict) -> None:
    task.setdefault("priority", "P2")
    task.setdefault("size_points", 5)
    task.setdefault("status", "backlog")
    task.setdefault("owner", DEFAULT_OWNER)
    task.setdefault("big_task", None)
    task.setdefault("success_criteria", [])
    task.setdefault("failure_criteria", [])
    task.setdefault("blockers", [])
    task.setdefault("dependencies", [])
    task.setdefault("conflicts", [])
    task.setdefault("comments", [])


@dataclass(slots=True)
class TaskSession:
    """Снимок доски задач внутри заблокированного контекста."""

    root: Path
    board_path: Path = field(default=BOARD_PATH, init=False)
    state_path: Path = field(default=STATE_PATH, init=False)
    log_path: Path = field(default=LOG_PATH, init=False)
    board: dict[str, Any] = field(init=False)
    assignments: dict[str, str] = field(init=False)
    _tasks_map: dict[str, dict] = field(init=False, default_factory=dict)
    _events: list[dict[str, Any]] = field(init=False, default_factory=list)
    _board_dirty: bool = field(init=False, default=False)
    _state_dirty: bool = field(init=False, default=False)

    def __post_init__(self) -> None:
        self.board = read_json(self.board_path, {"version": "v1", "tasks": []})
        self.board.setdefault("version", "v1")
        self.board.setdefault("tasks", [])
        for task in self.board["tasks"]:
            normalize_task(task)
        raw_assignments = read_json(self.state_path, {"assignments": {}})
        assignments = raw_assignments.get("assignments", {}) if isinstance(raw_assignments, dict) else {}
        valid_ids = {task.get("id") for task in self.board["tasks"]}
        self.assignments = {
            task_id: owner for task_id, owner in assignments.items() if task_id in valid_ids
        }

    # -- внутренняя механика -------------------------------------------------

    def mapping(self) -> dict[str, dict]:
        if not self._tasks_map:
            self._tasks_map = {task.get("id"): task for task in self.board.get("tasks", [])}
        return self._tasks_map

    def mark_board_dirty(self) -> None:
        self._board_dirty = True
        self.board["updated_at"] = now_iso()

    def mark_state_dirty(self) -> None:
        self._state_dirty = True

    def add_event(self, event: dict[str, Any]) -> None:
        self._events.append(event)

    def commit(self) -> None:
        if self._board_dirty:
            write_json_atomic(self.board_path, self.board)
        if self._state_dirty:
            write_json_atomic(self.state_path, {"assignments": self.assignments})
        if self._events:
            self.log_path.parent.mkdir(parents=True, exist_ok=True)
            with self.log_path.open("a", encoding="utf-8") as fh:
                for event in self._events:
                    fh.write(json.dumps(event, ensure_ascii=False) + "\n")

    # -- операции ------------------------------------------------------------

    def ensure_task(self, task_id: str) -> dict:
        task = self.mapping().get(task_id)
        if not task:
            raise SystemExit(f"Задача {task_id} не найдена")
        return task

    def update_assignment(self, task_id: str, owner: str) -> None:
        if owner == DEFAULT_OWNER:
            self.assignments.pop(task_id, None)
        else:
            self.assignments[task_id] = owner
        self.mark_state_dirty()

    def append_log_event(
        self,
        action: str,
        *,
        task: str,
        agent: str,
        note: str,
        previous_owner: str | None = None,
    ) -> None:
        event = {
            "action": action,
            "task": task,
            "agent": agent,
            "note": note,
            "timestamp": now_iso(),
        }
        if previous_owner is not None:
            event["previous_owner"] = previous_owner
        self.add_event(event)


@contextmanager
def task_session(*, exclusive: bool) -> Iterable[TaskSession]:
    with global_lock(exclusive=exclusive):
        session = TaskSession(SDK_ROOT)
        try:
            yield session
        finally:
            session.commit()


def dependency_status(task: dict, tasks_map: dict[str, dict]) -> str:
    deps = task.get("dependencies", [])
    if not deps:
        return ""
    rendered = []
    for dep_id in deps:
        dep = tasks_map.get(dep_id)
        if not dep:
            rendered.append(f"{dep_id}(missing)")
            continue
        rendered.append(f"{dep_id}({dep.get('status', 'unknown')})")
    return "Depends: " + ", ".join(rendered)


def blockers_status(task: dict) -> str:
    blockers = task.get("blockers", [])
    if blockers:
        return "Blockers: " + ", ".join(blockers)
    return ""


def conflicts_status(task: dict) -> str:
    conflicts = task.get("conflicts", [])
    if conflicts:
        return "Conflicts: " + ", ".join(conflicts)
    return ""


def success_lines(task: dict) -> list[str]:
    return ["Success> " + crit for crit in task.get("success_criteria", [])]


def failure_lines(task: dict) -> list[str]:
    return ["Failure> " + crit for crit in task.get("failure_criteria", [])]


def last_comment_line(task: dict) -> str:
    comments = task.get("comments", [])
    if not comments:
        return ""
    last = comments[-1]
    return f"Last comment: [{last['timestamp']}] {last['author']}: {last['message']}"


def read_history(path: Path, limit: int) -> list[dict]:
    if not path.exists() or limit <= 0:
        return []
    tail: deque[str] = deque(maxlen=limit)
    with path.open("r", encoding="utf-8", errors="ignore") as fh:
        for line in fh:
            tail.append(line)
    events: list[dict] = []
    for raw in tail:
        raw = raw.strip()
        if not raw:
            continue
        try:
            events.append(json.loads(raw))
        except json.JSONDecodeError:
            continue
    return events


def compute_summary(session: TaskSession, *, history_limit: int = 10) -> dict:
    tasks = session.board.get("tasks", [])
    counts = Counter(task.get("status", "unknown") for task in tasks)
    summary = {
        "generated_at": now_iso(),
        "board_version": session.board.get("version", "n/a"),
        "updated_at": session.board.get("updated_at"),
        "counts": {status: counts.get(status, 0) for status in STATUS_ORDER},
        "events": read_history(session.log_path, history_limit),
        "assignments": session.assignments.copy(),
    }
    tasks_map = session.mapping()
    candidates = [
        t
        for t in tasks
        if t.get("status") in {"ready", "backlog"}
        and session.assignments.get(t.get("id"), t.get("owner", DEFAULT_OWNER)) == DEFAULT_OWNER
    ]
    ordered = sorted(candidates, key=lambda t: (priority_rank(t), status_rank(t), tasks.index(t)))
    for task in ordered:
        deps = [dep for dep in task.get("dependencies", []) if tasks_map.get(dep, {}).get("status") not in {"done", "review"}]
        conflicts = [conf for conf in task.get("conflicts", []) if tasks_map.get(conf, {}).get("status") in {"in_progress", "review"}]
        if deps or conflicts:
            continue
        summary["next_task"] = {
            "id": task.get("id"),
            "title": task.get("title"),
            "priority": task.get("priority"),
        }
        break
    return summary


def compute_metrics(session: TaskSession) -> dict:
    summary = compute_summary(session, history_limit=100)
    tasks = session.board.get("tasks", [])
    tasks_map = session.mapping()
    wip_by_agent: dict[str, list[str]] = defaultdict(list)
    ready_unassigned = 0
    for task in tasks:
        task_id = task.get("id")
        owner = session.assignments.get(task_id, task.get("owner", DEFAULT_OWNER))
        if task.get("status") in {"in_progress", "review"} and owner != DEFAULT_OWNER:
            wip_by_agent[owner].append(task_id)
        if task.get("status") in {"ready", "backlog"} and owner == DEFAULT_OWNER:
            deps = [dep for dep in task.get("dependencies", []) if tasks_map.get(dep, {}).get("status") not in {"done", "review"}]
            conflicts = [conf for conf in task.get("conflicts", []) if tasks_map.get(conf, {}).get("status") in {"in_progress", "review"}]
            if not deps and not conflicts:
                ready_unassigned += 1

    events = read_history(session.log_path, 500)
    now = datetime.now(timezone.utc)
    assign_times: dict[str, datetime] = {}
    cycle_durations: list[float] = []
    throughput_24h = 0
    for event in events:
        ts = parse_time(event.get("timestamp"))
        if not ts:
            continue
        if event.get("action") in {"assign", "grab"}:
            assign_times[event.get("task", "")] = ts
        elif event.get("action") == "release":
            assign_times.pop(event.get("task", ""), None)
        elif event.get("action") == "complete":
            if ts >= now - timedelta(hours=24):
                throughput_24h += 1
            start = assign_times.get(event.get("task", ""))
            if start and ts >= start:
                cycle_durations.append((ts - start).total_seconds() / 3600)

    avg_cycle = sum(cycle_durations) / len(cycle_durations) if cycle_durations else None

    return {
        "generated_at": summary["generated_at"],
        "counts": summary["counts"],
        "next_task": summary.get("next_task"),
        "ready_unassigned": ready_unassigned,
        "throughput_24h": throughput_24h,
        "avg_cycle_time_hours": avg_cycle,
        "wip_by_agent": {agent: sorted(ids) for agent, ids in sorted(wip_by_agent.items())},
        "assignments": summary["assignments"],
    }


def print_task_list(session: TaskSession, *, compact: bool) -> None:
    summary = compute_summary(session)
    counts_line = " | ".join(
        f"{STATUS_TITLES.get(status, status)}={summary['counts'].get(status, 0)}"
        for status in STATUS_ORDER
    )
    print(f"Task Board — {summary['generated_at']}")
    print(f"Board version: {summary['board_version']} (updated_at {summary['updated_at']})")
    print("Summary: " + counts_line)
    print()

    groups: dict[str, list[dict]] = defaultdict(list)
    for task in session.board.get("tasks", []):
        groups[task.get("status", "backlog")].append(task)

    tasks_map = session.mapping()

    for status in STATUS_ORDER:
        items = groups.get(status, [])
        if not items:
            continue
        title = STATUS_TITLES.get(status, status)
        print(f"{title} ({len(items)}):")
        for task in sorted(items, key=lambda t: (priority_rank(t), t.get("id"))):
            owner = session.assignments.get(task.get("id"), task.get("owner", DEFAULT_OWNER))
            line = f"  - {task['id']} [{task.get('priority','P3')}] owner={owner}"
            if status in {"in_progress", "blocked", "review"}:
                line += " *focus"
            print(line)
            if compact:
                continue
            for extra in (
                dependency_status(task, tasks_map),
                blockers_status(task),
                conflicts_status(task),
            ):
                if extra:
                    print(f"      {extra}")
            for extra_line in success_lines(task):
                print(f"      {extra_line}")
            for extra_line in failure_lines(task):
                print(f"      {extra_line}")
            last = last_comment_line(task)
            if last:
                print(f"      {last}")
        print()

    if compact:
        return
    events = summary.get("events", [])[-5:]
    if events:
        print("Recent events:")
        for event in events:
            print(
                f"- {event.get('timestamp', '?')} — {event.get('agent', '?')} -> {event.get('task', '?')} "
                f"[{event.get('action', 'assign')}] {event.get('note', '')}"
            )


def print_conflicts(session: TaskSession) -> None:
    print("Task Conflicts Map:")
    for task in session.board.get("tasks", []):
        conflicts = task.get("conflicts", [])
        target = ", ".join(conflicts) if conflicts else "none"
        print(f"- {task['id']} -> {target}")


def validate_board(session: TaskSession) -> None:
    ids = [t.get("id") for t in session.board.get("tasks", [])]
    if len(ids) != len(set(ids)):
        raise SystemExit("Обнаружены дублирующиеся идентификаторы задач")
    tasks_map = session.mapping()
    for task in session.board.get("tasks", []):
        task_id = task.get("id")
        for dep in task.get("dependencies", []):
            if dep == task_id:
                raise SystemExit(f"Задача {task_id} зависит сама от себя")
            if dep not in tasks_map:
                raise SystemExit(f"Задача {task_id} зависит от отсутствующей задачи {dep}")
        for blocker in task.get("blockers", []):
            if blocker not in tasks_map:
                raise SystemExit(f"Задача {task_id} ссылается на отсутствующий blocker {blocker}")
        for conflict in task.get("conflicts", []):
            if conflict not in tasks_map:
                raise SystemExit(f"Задача {task_id} конфликтует с отсутствующей задачей {conflict}")
        if task.get("status") == "blocked" and not task.get("blockers"):
            raise SystemExit(f"Задача {task_id} помечена blocked без blockers")
    print("Task board validation passed")


def assign_task(session: TaskSession, task_id: str, agent: str, note: str, *, action: str, force: bool) -> None:
    task = session.ensure_task(task_id)
    tasks_map = session.mapping()
    if task.get("status") == "done" and not force:
        raise SystemExit(f"Задача {task_id} уже завершена; используйте FORCE=1")
    conflicts = []
    for conflict_id in task.get("conflicts", []):
        conflict = tasks_map.get(conflict_id)
        if conflict and conflict.get("status") in {"in_progress", "review"}:
            owner = session.assignments.get(conflict_id, conflict.get("owner", DEFAULT_OWNER))
            if owner not in {None, DEFAULT_OWNER, agent}:
                conflicts.append(f"{conflict_id} ({owner})")
    if conflicts and not force:
        raise SystemExit(f"Конфликты: {', '.join(conflicts)} — укажите FORCE=1")
    for dep_id in task.get("dependencies", []):
        dep = tasks_map.get(dep_id)
        if not dep:
            raise SystemExit(f"Несуществующая зависимость {dep_id}")
        if dep.get("status") not in {"done", "review"} and session.assignments.get(dep_id) not in {agent} and not force:
            raise SystemExit(f"Зависимость {dep_id} ещё не готова (status {dep.get('status')})")
    previous_owner = session.assignments.get(task_id, task.get("owner", DEFAULT_OWNER))
    session.update_assignment(task_id, agent)
    task["owner"] = agent
    if task.get("status") in {"backlog", "ready", "blocked"}:
        task["status"] = "in_progress"
    session.mark_board_dirty()
    session.append_log_event(action, task=task_id, agent=agent, note=note, previous_owner=previous_owner)
    print(f"Задача {task_id} назначена на {agent}. Предыдущий владелец: {previous_owner}")


def release_task(session: TaskSession, task_id: str, note: str) -> None:
    task = session.ensure_task(task_id)
    previous_owner = session.assignments.get(task_id, task.get("owner", DEFAULT_OWNER))
    session.update_assignment(task_id, DEFAULT_OWNER)
    task["owner"] = DEFAULT_OWNER
    if task.get("status") == "in_progress":
        task["status"] = "ready"
    session.mark_board_dirty()
    session.append_log_event("release", task=task_id, agent=previous_owner or DEFAULT_OWNER, note=note)
    print(f"Задача {task_id} освобождена (owner -> {DEFAULT_OWNER})")


def complete_task(session: TaskSession, task_id: str, agent: str, note: str) -> None:
    task = session.ensure_task(task_id)
    previous_owner = session.assignments.get(task_id, task.get("owner", DEFAULT_OWNER))
    session.update_assignment(task_id, DEFAULT_OWNER)
    task["owner"] = agent
    task["status"] = "done"
    session.mark_board_dirty()
    session.append_log_event("complete", task=task_id, agent=agent, note=note, previous_owner=previous_owner)
    print(f"Задача {task_id} отмечена как завершённая")


def comment_task(session: TaskSession, task_id: str, author: str, message: str) -> None:
    task = session.ensure_task(task_id)
    entry = {
        "author": author,
        "timestamp": now_iso(),
        "message": message,
    }
    task.setdefault("comments", []).append(entry)
    session.mark_board_dirty()
    session.append_log_event("comment", task=task_id, agent=author, note=message)
    print(f"Комментарий добавлен к {task_id}")


def grab_task(session: TaskSession, agent: str, note: str, *, force: bool) -> None:
    tasks = session.board.get("tasks", [])
    tasks_map = session.mapping()
    candidates = [
        t
        for t in tasks
        if t.get("status") in {"ready", "backlog"}
        and session.assignments.get(t.get("id"), t.get("owner", DEFAULT_OWNER)) == DEFAULT_OWNER
    ]
    ordered = sorted(candidates, key=lambda t: (priority_rank(t), status_rank(t), tasks.index(t)))
    for task in ordered:
        deps_blocking = [dep for dep in task.get("dependencies", []) if tasks_map.get(dep, {}).get("status") not in {"done", "review"}]
        conflicts = [conf for conf in task.get("conflicts", []) if tasks_map.get(conf, {}).get("status") in {"in_progress", "review"}]
        if (deps_blocking or conflicts) and not force:
            continue
        assign_task(session, task.get("id"), agent, note, action="grab", force=force)
        return
    print("Нет доступных задач для захвата")


def add_task(session: TaskSession, args: argparse.Namespace) -> None:
    tasks = session.board.setdefault("tasks", [])
    next_id = args.id
    if not next_id:
        existing = [t.get("id") for t in tasks if isinstance(t.get("id"), str) and t.get("id", "").startswith("T-")]
        max_num = 0
        for tid in existing:
            try:
                max_num = max(max_num, int(tid.split("-", 1)[1]))
            except Exception:
                continue
        next_id = f"T-{max_num + 1:03d}"
    if any(t.get("id") == next_id for t in tasks):
        raise SystemExit(f"Задача {next_id} уже существует")
    new_task = {
        "id": next_id,
        "title": args.title,
        "epic": args.epic,
        "status": args.status,
        "priority": args.priority,
        "size_points": args.size,
        "owner": DEFAULT_OWNER,
        "success_criteria": args.success,
        "failure_criteria": args.failure,
        "blockers": args.blockers,
        "dependencies": args.dependencies,
        "conflicts": args.conflicts,
        "big_task": args.big_task,
        "comments": [],
    }
    normalize_task(new_task)
    tasks.append(new_task)
    session._tasks_map = {}  # сброс кэша
    session.mark_board_dirty()
    session.append_log_event("add", task=next_id, agent=args.agent, note=args.note)
    print(f"Добавлена задача {next_id}: {args.title}")


def ensure_task_arg(value: str | None) -> str:
    task_id = value or os.environ.get("TASK")
    if not task_id:
        raise SystemExit("Укажите TASK=<id> или аргумент --task")
    return task_id


def ensure_agent(value: str | None) -> str:
    return value or os.environ.get("AGENT") or "gpt-5-codex"


def pick_note(value: str | None, default: str) -> str:
    return value or os.environ.get("NOTE") or default


def parse_csv(value: str | None) -> list[str]:
    if not value:
        return []
    return [item.strip() for item in value.split(",") if item.strip()]


def history_command(session: TaskSession, *, limit: int, json_output: bool) -> None:
    events = read_history(session.log_path, max(1, limit))
    if json_output:
        print(json.dumps(events, ensure_ascii=False))
    else:
        if not events:
            print("History is empty")
        for event in events:
            print(
                f"- {event.get('timestamp', '?')} — {event.get('agent', '?')} -> {event.get('task', '?')} "
                f"[{event.get('action', 'assign')}] {event.get('note', '')}"
            )


def summary_command(session: TaskSession, *, json_output: bool) -> None:
    summary = compute_summary(session)
    if json_output:
        print(json.dumps(summary, ensure_ascii=False))
        return
    counts_line = " | ".join(
        f"{STATUS_TITLES.get(status, status)}={summary['counts'].get(status, 0)}"
        for status in STATUS_ORDER
    )
    print(f"Task Board — {summary['generated_at']}")
    print(f"Board version: {summary['board_version']} (updated_at {summary['updated_at']})")
    print("Summary: " + counts_line)
    if summary.get("next_task"):
        nt = summary["next_task"]
        print(f"Next task: {nt['id']} ({nt['priority']}) — {nt['title']}")
    events = summary.get("events", [])[-5:]
    if events:
        print("Recent events:")
        for event in events:
            print(
                f"- {event.get('timestamp', '?')} — {event.get('agent', '?')} -> {event.get('task', '?')} "
                f"[{event.get('action', 'assign')}] {event.get('note', '')}"
            )


def metrics_command(session: TaskSession, *, json_output: bool) -> None:
    metrics = compute_metrics(session)
    if json_output:
        print(json.dumps(metrics, ensure_ascii=False))
        return
    print(f"Metrics — {metrics['generated_at']}")
    counts_line = " | ".join(
        f"{STATUS_TITLES.get(status, status)}={metrics['counts'].get(status, 0)}"
        for status in STATUS_ORDER
    )
    print("Counts: " + counts_line)
    print(f"Ready unassigned: {metrics['ready_unassigned']}")
    print(f"Throughput 24h: {metrics['throughput_24h']}")
    avg_cycle = metrics.get("avg_cycle_time_hours")
    if avg_cycle is not None:
        print(f"Avg cycle time (h): {avg_cycle:.2f}")
    next_task = metrics.get("next_task")
    if next_task:
        print(f"Next task: {next_task['id']} ({next_task['priority']}) — {next_task['title']}")
    if metrics["wip_by_agent"]:
        print("WIP by agent:")
        for agent, ids in metrics["wip_by_agent"].items():
            print(f"  - {agent}: {len(ids)} -> {', '.join(ids)}")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="task", description="Task board control plane")
    sub = parser.add_subparsers(dest="command", required=True)

    list_parser = sub.add_parser("list", help="Показать доску задач")
    list_parser.add_argument("--compact", action="store_true")

    status_parser = sub.add_parser("status", help="Синоним list")
    status_parser.add_argument("--compact", action="store_true")

    summary_parser = sub.add_parser("summary", help="Краткое резюме доски")
    summary_parser.add_argument("--json", action="store_true")

    metrics_parser = sub.add_parser("metrics", help="Метрики эффективности")
    metrics_parser.add_argument("--json", action="store_true")

    sub.add_parser("conflicts", help="Карта конфликтов")

    assign_parser = sub.add_parser("assign", help="Назначить задачу агенту")
    assign_parser.add_argument("--task")
    assign_parser.add_argument("--agent")
    assign_parser.add_argument("--note")
    assign_parser.add_argument("--force", action="store_true")

    select_parser = sub.add_parser("select", help="Алиас assign")
    select_parser.add_argument("--task")
    select_parser.add_argument("--agent")
    select_parser.add_argument("--note")
    select_parser.add_argument("--force", action="store_true")

    grab_parser = sub.add_parser("grab", help="Автозахват доступной задачи")
    grab_parser.add_argument("--agent")
    grab_parser.add_argument("--note")
    grab_parser.add_argument("--force", action="store_true")

    release_parser = sub.add_parser("release", help="Освободить задачу")
    release_parser.add_argument("--task")
    release_parser.add_argument("--note")

    complete_parser = sub.add_parser("complete", help="Завершить задачу")
    complete_parser.add_argument("--task")
    complete_parser.add_argument("--agent")
    complete_parser.add_argument("--note")

    comment_parser = sub.add_parser("comment", help="Добавить комментарий")
    comment_parser.add_argument("--task")
    comment_parser.add_argument("--author")
    comment_parser.add_argument("--message")

    validate_parser = sub.add_parser("validate", help="Проверить целостность")
    validate_parser.set_defaults()

    history_parser = sub.add_parser("history", help="История событий")
    history_parser.add_argument("--limit", type=int, default=int(os.environ.get("LIMIT", "10")))
    history_parser.add_argument("--json", action="store_true")

    add_parser = sub.add_parser("add", help="Добавить задачу")
    add_parser.add_argument("--title")
    add_parser.add_argument("--epic")
    add_parser.add_argument("--priority")
    add_parser.add_argument("--size", type=float)
    add_parser.add_argument("--status")
    add_parser.add_argument("--blockers")
    add_parser.add_argument("--dependencies")
    add_parser.add_argument("--conflicts")
    add_parser.add_argument("--success")
    add_parser.add_argument("--failure")
    add_parser.add_argument("--big-task")
    add_parser.add_argument("--id")
    add_parser.add_argument("--agent")
    add_parser.add_argument("--note")

    return parser


def main(argv: list[str] | None = None) -> int:
    argv = argv if argv is not None else sys.argv[1:]
    parser = build_parser()
    args = parser.parse_args(argv)

    command = args.command

    if command in {"list", "status", "summary", "conflicts", "history", "metrics"}:
        with task_session(exclusive=False) as session:
            if command in {"list", "status"}:
                print_task_list(session, compact=getattr(args, "compact", False))
            elif command == "summary":
                summary_command(session, json_output=args.json)
            elif command == "metrics":
                metrics_command(session, json_output=args.json)
            elif command == "conflicts":
                print_conflicts(session)
            elif command == "history":
                json_output = args.json or os.environ.get("JSON", "0").lower() in {"1", "true", "yes", "on"}
                history_command(session, limit=args.limit, json_output=json_output)
        return 0

    if command == "validate":
        with task_session(exclusive=False) as session:
            validate_board(session)
        return 0

    if command in {"assign", "select", "grab", "release", "complete", "comment", "add"}:
        with task_session(exclusive=True) as session:
            if command in {"assign", "select"}:
                task_id = ensure_task_arg(args.task)
                agent = ensure_agent(args.agent)
                note = pick_note(args.note, "manual assign")
                force = args.force or bool(os.environ.get("FORCE"))
                assign_task(session, task_id, agent, note, action="assign", force=force)
            elif command == "grab":
                agent = ensure_agent(args.agent)
                note = pick_note(args.note, "auto-grab")
                force = args.force or os.environ.get("FORCE", "0").lower() in {"1", "true", "yes", "on"}
                grab_task(session, agent, note, force=force)
            elif command == "release":
                task_id = ensure_task_arg(args.task)
                note = pick_note(args.note, "release")
                release_task(session, task_id, note)
            elif command == "complete":
                task_id = ensure_task_arg(args.task)
                agent = ensure_agent(args.agent)
                note = pick_note(args.note, "complete")
                complete_task(session, task_id, agent, note)
            elif command == "comment":
                task_id = ensure_task_arg(args.task)
                message = args.message or os.environ.get("MESSAGE")
                if not message:
                    raise SystemExit("Укажите MESSAGE=... или аргумент --message")
                author = args.author or os.environ.get("AUTHOR") or "gpt-5-codex"
                comment_task(session, task_id, author, message)
            elif command == "add":
                title = args.title or os.environ.get("TITLE")
                if not title:
                    raise SystemExit("Укажите --title или переменную TITLE для новой задачи")
                args.title = title
                args.epic = args.epic or os.environ.get("EPIC", "default")
                args.priority = args.priority or os.environ.get("PRIORITY", "P1")
                size_value = args.size if args.size is not None else os.environ.get("SIZE")
                if isinstance(size_value, str):
                    try:
                        size_value = float(size_value)
                    except ValueError as exc:  # pragma: no cover - простая валидация
                        raise SystemExit("SIZE должен быть числом") from exc
                if size_value is None:
                    size_value = 5
                args.size = int(round(size_value))
                args.status = args.status or os.environ.get("STATUS", "backlog")
                args.blockers = parse_csv(args.blockers or os.environ.get("BLOCKERS"))
                args.dependencies = parse_csv(args.dependencies or os.environ.get("DEPENDENCIES"))
                args.conflicts = parse_csv(args.conflicts or os.environ.get("CONFLICTS"))
                args.success = parse_csv(args.success or os.environ.get("SUCCESS"))
                args.failure = parse_csv(args.failure or os.environ.get("FAILURE"))
                args.big_task = args.big_task or os.environ.get("BIG_TASK")
                args.agent = ensure_agent(args.agent or os.environ.get("AGENT"))
                args.note = args.note or os.environ.get("NOTE") or "add task"
                add_task(session, args)
        return 0

    raise SystemExit(f"Неизвестная команда task: {command}")


if __name__ == "__main__":  # pragma: no cover - точка входа
    sys.exit(main())

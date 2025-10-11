# Shelldone Worklog

Вместо legacy `.agentcontrol` файлов используем открытую документацию:

- Статусы и текущие результаты — см. `docs/status.md`.
- Полный список задач и критерии — см. `docs/tasks.yaml`.
- Архитектурное видение и бюджеты — см. `docs/architecture/manifest.md`.
- Процесс обновления статусов — см. `docs/governance/status-updates.md`.

Этот файл оставлен как указатель, чтобы инструменты и люди, ожидающие `todo.machine.md`, могли быстро найти новую структуру.

## Program
```yaml
program: Shelldone Terminal Platform
program_id: sdp-q4-2025
name: Shelldone Terminal Platform — Q4 2025
updated_at: 2025-10-09T00:00:00Z
objectives:
  - Stabilize TermBridge orchestration
  - Harden QA pipeline and perf budgets
kpis:
  - termbridge_matrix_time_min: 12
  - diff_cover_pct: 95
progress_pct: 0
health: green
milestones:
  - title: QA Hardening
    due: 2025-10-31
    status: in_progress
  - title: Plugin Platform MVP
    due: 2025-11-30
    status: planned
policies:
  - all releases pass RTF gate
  - perf regressions >5% block merge
```

## Epics
```yaml
id: epic-termbridge
title: TermBridge Core
type: epic
status: in_progress
priority: P0
size_points: 40
scope_paths:
  - shelldone-agentd/src/app/termbridge
  - scripts/tests/termbridge_matrix.py
spec: docs/architecture/termbridge.md
budgets:
  - spawn_p95_ms<=250
risks:
  - terminal_ipc_drift
dependencies:
  - utif-sigma
big_tasks_planned:
  - task-termbridge-matrix
progress_pct: 0
health: green
tests_required:
  - scripts/tests/termbridge_matrix.py
verify_commands:
  - VERIFY_MODE=prepush make verify
docs_updates:
  - docs/architecture/termbridge.md
  - docs/ROADMAP/2025Q4.md
artifacts:
  - dashboards/artefacts/termbridge/linux.json
audit:
  - reviewer: imagray
    date: 2025-10-09
```

## Big Tasks
```yaml
id: task-termbridge-matrix
title: TermBridge Matrix + Snapshot
parent_epic: epic-termbridge
type: test
priority: P1
size_points: 8
status: planned
scope_paths:
  - scripts/tests/termbridge_matrix.py
spec: docs/architecture/termbridge.md
budgets:
  - matrix_runtime_min<=15
risks:
  - drift_in_discovery
dependencies:
  - shelldone-agentd
progress_pct: 0
health: green
tests_required:
  - scripts/tests/termbridge_matrix.py
verify_commands:
  - python3 scripts/tests/termbridge_matrix.py
docs_updates:
  - docs/ROADMAP/2025Q4.md
artifacts:
  - dashboards/artefacts/termbridge/linux.json
audit:
  - reviewer: imagray
    date: 2025-10-09
```

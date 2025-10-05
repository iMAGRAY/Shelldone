# AGENTS.md — Orientation for Shelldone AI Agents

## Where to Find Knowledge
- **Architecture overview:** `docs/architecture/README.md`
- **Plugin model & IDE layers:** `docs/architecture/customization-and-plugins.md`
- **AI & MCP integration:** `docs/architecture/ai-integration.md`
- **Animation & visual effects:** `docs/architecture/animation-framework.md`
- **Performance budgets:** `docs/architecture/perf-budget.md`
- **State & backups:** `docs/architecture/state-and-storage.md`
- **Security & secrets:** `docs/architecture/security-and-secrets.md`
- **Observability:** `docs/architecture/observability.md`
- **Release & compatibility:** `docs/architecture/release-and-compatibility.md`
- **Roadmap:** `docs/ROADMAP/2025Q4.md` plus notes in `docs/ROADMAP/notes/`
- **Delivery process:** `docs/community/contributor-handbook.md`, `CONTRIBUTING.md`
- **Recipes & workflows:** `docs/recipes/animations.md`, `docs/recipes/plugins.md`, `docs/recipes/observability.md`, `docs/recipes/workflows.md`

## Quality & Pipelines
- Primary entry point: `make verify` with `VERIFY_MODE=fast|prepush|full|ci`.
- Shortcut targets: `make verify-prepush`, `make verify-ci` (details in README/handbook).
- Artifacts live in `artifacts/verify/` and `artifacts/perf/` (see perf budget doc).
- Baselines: `qa/baselines/banned_markers.json` and `qa/baselines/clippy.json`.
  Refresh via `python3 scripts/verify.py --update-*-baseline` only after deliberate cleanup.
- Roadmap health: `make roadmap status` (strict threshold ±0.5 p.p., supports `JSON=1`, `STRICT=0`).

## Agentcall Operations

### Interface Basics
- `agentcall <command> [project-path] -- [extra args]` — always pass the Shelldone root (`.`) as the project path when a command expects further arguments (for example `agentcall run task . -- summary`).
- Sub-commands are defined in `agentcontrol/agentcall.yaml`; reports land under `agentcontrol/reports/` and are copied to `reports/` when relevant.
- Re-run a command if the CLI self-updates mid-execution; telemetry will show paired `auto-update` → original command.

### Pre-flight Sequence (every session)
- `agentcall telemetry tail --limit 3` — inspect the most recent events; ensure the latest `auto-update` is followed by a successful `run`/`pipeline` entry.
- `agentcall --version` **must** equal `cat agentcontrol/state/tooling.lock`; mismatch → `agentcall self-update` then re-check.
- `git status --short` remains clean before pipelines; commit/stash first so automation can rewrite manifests safely.
- `python3 scripts/project_health_check.py` (or `make health-check`) — exits ≠0 when progress drift, stale task board (>24 h), stale Memory Heart (>6 h), or unreadable approvals are detected.

### Environment Bootstrap
- `make setup-env` provisions the Python venv, installs pinned requirements, and (unless `SKIP_AGENT_INSTALL=1`) rebuilds CLI agent binaries under `agentcontrol/scripts/bin/`.
- `agentcall agents . install` followed by `agentcall agents . auth` seeds agent credentials; confirm with `agentcall agents . plan <task-id> codex "<Role>" --dry-run` (falls back to loopback mode if binaries missing).
- Memory Heart: `agentcall heart . sync` after large doc/code changes or at least every 6 h; use `agentcall heart . query "<term>" --top-k 5` to confirm index freshness.

### Daily Control Loop
1. `agentcall status` → runs `scripts/progress.py`, persists manifest/todo/status snapshots, then `scripts/roadmap-status.sh` and `scripts/task.sh summary`. Treat any warnings as release blockers.
2. `agentcall run progress . -- --dry-run` shows computed vs manual metrics without writing files; ensure warnings list is empty before committing.
3. Task board hygiene via `agentcall run task . summary`, `agentcall run task . assign --task <id> --agent <owner>`, and `agentcall run task . validate`.
4. `agentcall verify` with `VERIFY_MODE=fast|prepush|full|ci` (combine with `CHANGED_ONLY=1`, `NET=0/1`); watchdogы: \[`VERIFY_STEP_TIMEOUT_SEC`=900s, `VERIFY_TOTAL_TIMEOUT_SEC`=7200s\] по умолчанию, настраиваются через env.
5. Diff coverage включён автоматически (`diff-cover` >=90% по изменённым строкам). Поднять/понизить порог → `VERIFY_DIFF_COVER_THRESHOLD=<value>`.
6. `agentcall fix` (safe idempotent autofixes) → rerun `agentcall verify` immediately.
7. Pre-merge/release: `agentcall review` (diff-focused checks) and `agentcall ship` (locks deps, SBOM/SCA, release gate).

### Telemetry & Drift Guard
- `agentcall telemetry report` prints aggregated event counts; use it alongside `agentcall telemetry tail` for trend analysis.
- Progress engine warnings cover manual/computed drift (>5 p.p.), missing tasks, stale milestones. Resolve **before** updating roadmap docs manually.
- `reports/status.json` mirrors `agentcontrol/reports/status.json`; treat the copied timestamp as the source of truth.
- Memory Heart SLA: regenerate when `scripts/project_health_check.py --json` reports `heart.age_hours > 6`. Automate via cron if needed.

### Agent & Workflow Helpers
- `agentcall agents . assign <task-id> codex "<Role>" --dry-run` (and `plan`/`analysis`) orchestrates prompts; without configured binaries it returns a “loopback” JSON recommendation while still annotating the task board.
- `agentcall agents . logs` shows recent run artifacts under `reports/agents/`; use `agentcall agents . workflow --help` for batch task execution.
- `agentcall run task . history --task <id>` provides audit trail; `comment` appends structured notes (timestamped, owner-updating).

### Memory Heart Toolkit
- `agentcall heart . sync` (or `refresh`) indexes the repo; `agentcall heart . serve --port 8765` provides an HTTP endpoint during intense debugging sessions.
- `agentcall heart . update` keeps the heart submodule pinned; record the new commit in `agentcontrol/context/heart/manifest.json` automatically via sync.

### Troubleshooting
- `agentcall doctor` writes `agentcontrol/reports/doctor.json` and highlights missing toolchain dependencies; re-run until zero problems remain.
- If a command complains about “path … does not contain an AgentControl capsule”, prepend the project root: e.g. `agentcall run task . summary`.
- Permission issues under `agentcontrol/state/` or `agentcontrol/reports/` → fix with `chown -R $USER:$USER agentcontrol/state agentcontrol/reports` (no sudo inside CI).
- After an auto-update restart, rerun the original command and verify telemetry produced paired events.
- Watchdog tuning: экспортируйте `VERIFY_STEP_TIMEOUT_SEC`, `VERIFY_TOTAL_TIMEOUT_SEC`, `VERIFY_DIFF_COVER_THRESHOLD` при необходимости более жёстких/мягких лимитов.
- Invoke custom SDK hooks with `agentcall run <pipeline> . -- <args>` instead of calling scripts directly; this preserves telemetry and governance rules.

### Quick Reference
| Command | Purpose | Key files |
| --- | --- | --- |
| `agentcall status` | Recompute progress, sync roadmap/task board, copy status JSON | `agentcontrol/scripts/status.sh`, `scripts/progress.py` |
| `agentcall run progress . -- --dry-run` | Inspect computed progress without writing | `agentcontrol/scripts/progress.py` |
| `agentcall verify` | Golden gate (fmt/lint/tests/SCA/perf/docs) | `agentcontrol/scripts/verify.sh`, `reports/verify.json` |
| `VERIFY_STEP_TIMEOUT_SEC=1800 agentcall verify` | Расширение watchdog-предела для тяжёлых прогонов | `agentcontrol/scripts/verify.sh` |
| `agentcall fix` | Auto-fixes then re-run verify manually | `agentcontrol/scripts/fix.sh` |
| `agentcall review` | Diff-focused QA for changed files | `agentcontrol/scripts/review.sh` |
| `agentcall ship` | Release gate + SBOM/lock enforcement | `agentcontrol/scripts/ship.sh` |
| `agentcall run task . summary` | Board snapshot (counts, WIP) | `scripts/lib/sdklib/task_cli.py` |
| `agentcall agents . assign <task-id> codex "<Role>" --dry-run` | Generate agent prompt (loopback fallback if CLI missing) | `agentcontrol/scripts/agents/run.py`, `agentcontrol/scripts/agents/context.py` |
| `agentcall telemetry tail --limit 20` | Recent telemetry events (auto-update/run/pipeline) | AgentControl telemetry store |
| `python3 scripts/project_health_check.py --json` | Drift guard (progress, task board, heart, approvals) | `scripts/project_health_check.py` |
| `agentcall heart . sync` | Refresh Memory Heart index | `agentcontrol/scripts/agents/heart_engine.py` |
| `agentcall doctor` | Toolchain diagnostics | `agentcontrol/scripts/doctor.sh` |

## Operating Procedure for GPT-5 Codex
1. Inspect workspace state with `git status --short` to avoid conflicts.
2. Plan using `make roadmap status` (optionally `JSON=1`) and align work with `todo.machine.md`.
3. Before coding, understand which `.rs` files changed (verify will skip fmt when nothing changed).
4. Implement according to the architecture docs (state, security, observability, release are mandatory).
5. Run pipelines: at minimum `make verify-fast`; before pushing → `make verify-prepush`; before release → `make verify-full` and `make verify-ci`.
6. Update progress: edit `todo.machine.md` (progress_pct, tasks) and `docs/ROADMAP/2025Q4.md` to reflect real status.
7. Manage baselines: resolve new TODO/FIXME/lints or update baselines with `scripts/verify.py --update-marker-baseline` / `--update-clippy-baseline`.
8. Document every architectural or behavioural change in the relevant file (architecture, recipes, security runbook). Outdated docs should trigger verify/roadmap checks.

## Planning & Decision Records
- Sync epic/task status with `docs/ROADMAP/2025Q4.md`.
- Record architecture decisions via ADRs in `docs/architecture/adr/`.
- Draft proposals in `docs/ROADMAP/notes/` before discussion.

## Useful Directories
- Configuration & themes: `config/`
- Plugins and templates: `plugins/` (see `plugins/README.md`)
- Automation/perf scripts: `scripts/` (structure outlined in `docs/architecture/perf-budget.md`)
- Recipes and how-tos: `docs/recipes/`

Always update the relevant documents before merging changes so agents and humans rely on fresh sources.

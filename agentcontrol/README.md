# GPT-5 Codex SDK Toolkit

Enterprise-grade orchestration for autonomous coding agents. The toolkit ships a curated command surface, deterministic planning artefacts, and turnkey agent runtimes so GPT-class developers land in a predictable environment within seconds.

## Key Outcomes
- **Deterministic workflows** — single-entry CLI (`agentcall init/verify/fix/review/ship/status`) обеспечивает одинаковые пайплайны для людей и агентов.
- **Integrated governance** — roadmaps, task boards, and architectural manifests stay synchronized through `agentcall progress`, `agentcall status`, and `agentcall run architecture-sync`.
- **Agent-ready runtime** — Codex and Claude CLIs install, authenticate, and execute inside the project sandbox, delegating work or reviews with zero manual prep.
- **Knowledge fabric** — Memory Heart builds a local vector index of source and docs so agents and humans can query the entire codebase with millisecond latency.
- **Compliance guardrails** — lockfiles, SBOMs, quality gates, and audit logs keep delivery reproducible and verifiable.

## Quick Start
1. **Install prerequisites** (Bash ≥ 5.0, Python ≥ 3.10, Cargo ≥ 1.75, Node.js ≥ 18). Ensure `sudo` is available if system packages must be installed.
2. **Global install** (один раз на машине):
   ```bash
   ./scripts/install_agentcontrol.sh
   pip install -e .  # либо pipx install .
   ```
   Скрипт кладёт шаблоны в `~/.agentcontrol/templates/stable/0.2.0`, а `pip install` добавляет `agentcall` в `PATH`.
3. **Bootstrap проекта:**
   ```bash
   agentcall init ~/workspace/my-project
   ```
   Команда разворачивает структуру проекта, генерирует `config/commands.sh`, `agentcontrol/agentcall.yaml`, дорожную карту и отчёты.
4. **Аутентифицируйте агентов (из корня проекта):**
   ```bash
   cd ~/workspace/my-project
   agentcall agents auth
   agentcall agents status
   ```
   CLI запустит Codex/Claude login, сохранит токены в `~/.agentcontrol/state/`.
5. **Проверка окружения:**
   ```bash
   agentcall verify
   ```
   Пайплайн выполнит форматирование, тесты, безопасность, синхронизацию архитектуры и соберёт отчёты.

## Command Portfolio
| Command | Purpose | Notes |
| --- | --- | --- |
| `agentcall setup` | Install required system packages, Python/Node deps, and agent CLIs. | Use `SKIP_AGENT_INSTALL=1` or `SKIP_HEART_SYNC=1` to shorten bootstrap on air-gapped hosts. |
| `agentcall init` | Generate command hooks, roadmap/task board baselines, and status reports. | Idempotent; safe to rerun after upgrades. |
| `agentcall dev` | Print the quick reference (from `AGENTS.md`) and start configured dev commands. | Respects overrides in `config/commands.sh`. |
| `agentcall verify` | Canonical quality gate (format, lint, tests, coverage, security, docs, roadmap/task board validation, Memory Heart check). | Supports `VERIFY_MODE=prepush|ci|full`, `CHANGED_ONLY=1`, `NET=0|1`, `TIMEOUT_MIN=<n>`, `QUIET=1`, `JSON=1`. |
| `agentcall fix` | Execute safe autofixes defined in `SDK_FIX_COMMANDS`. | Follow with `agentcall verify` before committing. |
| `agentcall review` | Diff-focused review workflow (`SDK_REVIEW_LINTERS`, `SDK_TEST_COMMAND`, optional `diff-cover`). | Outputs `reports/review.json`; accepts `REVIEW_BASE_REF`, `REVIEW_SAVE`, `REVIEW_FORMAT`. |
| `agentcall ship` | Release gate: runs verify in pre-push mode, bumps version (`BUMP=patch|minor|major`), updates changelog, tags and pushes. | Aborts if any gate fails or open micro tasks exist. |
| `agentcall status` | Comprehensive dashboard (Program/Epics/Big Tasks, roadmap phases, task board summary, Memory Heart state). | Invokes `agentcall progress` automatically before rendering tables. |
| `agentcall roadmap` | Phase-focused report with formal progress tables and deltas. | Uses the same progress engine as `agentcall progress`. |
| `agentcall progress` | Parse `architecture/manifest.yaml` and `todo.machine.md`, recompute weighted progress, sync YAML blocks, and persist audit metadata. | Runs in dry-run mode with `DRY_RUN=1`. |
| `agentcall agents install` | Build Codex CLI from `vendor/codex` (Cargo) and install Claude CLI into `scripts/bin/`. | Falls back to system binaries when sandbox install fails. |
| `agentcall agents auth` | Authenticate all configured agent CLIs and store credentials in the sandbox state directory. | Skips already authenticated agents and reminds about `agentcall agents logout`. |
| `agentcall agents status` | Display health of agent binaries, credentials, and last activity. | Useful for CI smoke tests. |
| `agentcall heart sync` | Refresh the Memory Heart vector index. | Query with `agentcall heart query Q="…"` or expose an API via `agentcall heart serve`. |

## Agent Operations
### Installing and Updating CLIs
- `agentcall run vendor-update` pulls upstream submodules (Codex CLI, Claude Code, Memory Heart).
- `agentcall agents install` compiles Codex (Rust) and installs Claude (Node). Artifacts land in `scripts/bin/`. Installation logs are stored in `reports/agents/install.timestamp`.

### Authenticating Agents
- Run `agentcall agents auth` in the project root. Successful logins persist JSON credentials in `state/agents/<agent>/`. CLI prompts close automatically once tokens are captured.
- To rotate credentials: `agentcall agents logout` removes stored tokens and marks the status as `logged_out`.

### Delegating Work
- `agentcall run agent-assign TASK=T-123 [AGENT=codex] [ROLE="Implementation Lead"]` prepares contextual bundles (git diff, Memory Heart excerpts, roadmap slices) and streams them to the chosen CLI.
- `agentcall run agent-plan TASK=T-123` or `agentcall run agent-analysis` request planning or diagnostic summaries.
- Workflow pipelines combine assign + review steps via `agentcall agents workflow --task=T-123 [--workflow=default]`. Configure defaults in `config/agents.json`.
- Inspect agent activity with `agentcall agents logs [AGENT=claude] [LAST=20]` and verify readiness via `agentcall agents status`.

### Sandbox Execution
Agent processes run through `scripts/agents/run.py`, which wraps binaries inside bubblewrap when available (fallback: direct execution). Adjust sandbox profiles in `config/agents.json` per agent if custom isolation is required.

## Memory Heart
- Configuration lives in `config/heart.json`; state is rooted at `state/heart/`.
- `agentcall heart sync` updates embeddings incrementally; use `SKIP_HEART_SYNC=1` during bootstrap to defer large syncs.
- `agentcall heart query Q="build pipeline"` prints top-matching chunks with file and line references.
- `agentcall heart serve` exposes the index via HTTP for real-time agent consumption.

## Planning and Governance
- `todo.machine.md` maintains Program → Epics → Big Tasks (no micro tasks). It is regenerated by `agentcall progress` and `agentcall run architecture-sync`.
- `data/tasks.board.json`, `state/task_state.json`, and `journal/task_events.jsonl` capture the operational task board. Manage entries via `agentcall task add/take/drop/done/status/conflicts/metrics/history`.
- `architecture/manifest.yaml` is the source of truth for program metadata, systems, tasks, ADR/RFC references, and roadmap phases. `agentcall run architecture-sync` regenerates docs/ADR/RFC/roadmap artefacts from this manifest.
- Micro tasks belong exclusively to the Update Plan Tool (UPT) inside the Codex CLI; ensure the queue is empty before running `agentcall ship`.

## Customising Commands
Edit `config/commands.sh` to align the toolkit with the target stack:
```bash
SDK_VERIFY_COMMANDS=("npm run lint" "npm test")
SDK_FIX_COMMANDS=("npm run lint -- --fix")
SDK_SHIP_COMMANDS=("npm run build" "npm publish")
```
Commands execute sequentially; any non-zero exit aborts the current phase. All scripts use `set -Eeuo pipefail` for predictable failure handling.

## Continuous Integration
- GitHub Actions loads `.github/workflows/ci.yml`, which runs `agentcall verify` on push/PR and nightly at 03:00 UTC. SARIF findings surface in GitHub Security alerts.
- `agentcall status` and `agentcall roadmap` are safe to publish as artefacts for leadership dashboards.
- Pre-push hooks should call `agentcall verify --mode=prepush CHANGED_ONLY=1 JSON=1` to block regressions early.

## Troubleshooting
- **Permission denied inside `state/`** — ensure the project root is writable. When running inside restricted directories, export `AGENTCONTROL_STATE_DIR=/custom/path` before invoking commands.
- **Third-party installs are slow** — set `SKIP_AGENT_INSTALL=1` and run `agentcall agents install` later. Cache `~/.cache/pip` and `~/.cache/npm` in CI.
- **Memory Heart syncs are heavy** — run `agentcall heart sync DRY_RUN=1` to estimate impact, or configure path globs in `config/heart.json` to reduce scope.
- **Agents reuse global credentials** — verify that `agentcall agents auth` created files under `state/agents/`. Run `agentcall agents logout` followed by `agentcall agents auth` to regenerate sandboxed credentials.
- **Quality gates fail on placeholders** — update `config/commands.sh` with real commands; the SDK promotes safe defaults when placeholders remain.

## Support & Change Control
- Source of truth for governance: `AGENTS.md`, `architecture/manifest.yaml`, `todo.machine.md`, and `data/tasks.board.json`.
- Architectural decisions live in `docs/adr/`; RFC drafts in `docs/rfc/`; change log seeds in `docs/changes.md`.
- Submit pull requests with `agentcall verify` output attached. For high-risk modifications, pair with an agent-driven review via `agentcall agents workflow`.

For questions or escalations contact the owners listed in `AGENTS.md`.

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

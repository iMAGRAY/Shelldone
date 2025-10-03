# Shelldone Architecture Overview

This document outlines the target architecture for Shelldone. Detailed specifications live in the thematic files inside this directory and in `docs/ROADMAP/`.

## Core Principles
- **Performance first.** Every component must stay within the CPU/GPU/memory budgets defined in `docs/architecture/perf-budget.md`.
- **Quality by default.** All changes must pass `make verify` (see `docs/community/contributor-handbook.md` for modes and expectations).
- **Open extension points.** New functionality is implemented through plugin APIs and documented across `docs/architecture/`.
- **Native AI support.** People and agents share one control protocol; automation is a first-class citizen.

## Thematic Specifications
- `docs/architecture/customization-and-plugins.md` — plugin model, themes, hooks, and IDE capabilities.
- `docs/architecture/ai-integration.md` — MCP/AI interaction protocols, агентные адаптеры (OpenAI Agents SDK, Claude Agent SDK) и политика обновлений.
- `docs/architecture/animation-framework.md` — high-performance effects and animation system.
- `docs/architecture/perf-budget.md` — performance budgets, profiling, and fallback modes.
- `docs/architecture/state-and-storage.md` — lifecycle of state, snapshots, sync, and backups.
- `docs/architecture/security-and-secrets.md` — threat model, sandboxing, secret storage, and policies.
- `docs/architecture/observability.md` — metrics, logging, tracing, SLOs, and alerting.
- `docs/architecture/release-and-compatibility.md` — release engineering, auto-update, migrations, and API compatibility.

## Roadmap
Quarterly phases and milestones are defined in `docs/ROADMAP/2025Q4.md`; supporting notes live in `docs/ROADMAP/notes/`.

All architectural decisions require an ADR (`docs/architecture/adr/`). If a new change lacks an ADR, create one before implementation.

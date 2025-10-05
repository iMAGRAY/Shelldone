# Shelldone Architecture Overview

This document outlines the target architecture for Shelldone. Detailed specifications live in the thematic files inside this directory and in `docs/ROADMAP/`.

## Core Principles
- **Performance first.** Every component must stay within the CPU/GPU/memory budgets defined in `docs/architecture/perf-budget.md`.
- **Quality by default.** All changes must pass `make verify` (see `docs/community/contributor-handbook.md` for modes and expectations).
- **Open extension points.** New functionality is implemented through plugin APIs and documented across `docs/architecture/`.
- **Native AI support.** People and agents share one control protocol; automation is a first-class citizen.

## Thematic Specifications
- `docs/architecture/utif-sigma.md` — Unified Terminal Intelligence Fabric (Σ-pty/Σ-json/Σ-cap), ACK протокол, Continuum snapshots, policy и perf SLA.
- `docs/architecture/customization-and-plugins.md` — plugin model, themes, hooks, and IDE capabilities.
- `docs/architecture/ai-integration.md` — MCP automation fabric, discovery, context schema, adapters, security (TLS/mTLS, горячая ротация, cipher policies).
- `docs/architecture/agent-sdk-bridge.md` — единый мост для OpenAI/Claude/Microsoft Agent SDK, lifecycle биндингов и политика комплаенса.
- `docs/architecture/persona-engine.md` — Persona Engine Nova/Core/Flux + Experience presets (Beginner/Ops/Expert), onboarding wizard и YAML-схема hint budgets.
- `docs/architecture/pain-matrix.md` — соответствие 25 болей Shelldone текущим возможностям, пробелам и roadmap-инициативам.
- `docs/architecture/plugin-marketplace.md` — Capability marketplace hooks, bundle lifecycle, σ-cap обновления.
- `docs/architecture/agent-governance.md` — управление адаптерами, секретами, observability и тестированием agentd.
- `docs/architecture/animation-framework.md` — high-performance effects and animation system.
- `docs/architecture/perf-budget.md` — performance budgets, profiling, and fallback modes.
- `docs/architecture/state-and-storage.md` — lifecycle of state, snapshots, sync, and backups.
- `docs/architecture/security-and-secrets.md` — threat model, sandboxing, secret storage, and policies.
- `docs/architecture/observability.md` — metrics, logging, tracing, SLOs, and alerting.
- `docs/architecture/release-and-compatibility.md` — release engineering, auto-update, migrations, and API compatibility.
- `docs/architecture/pain-matrix.md` — приоритизированные pain-points с привязкой к задачам (`task-*`).
- `docs/architecture/termbridge.md` — единый оркестратор терминалов, capability map schema, consent workflow и UX/obserability контуры.

## Roadmap
Quarterly phases and milestones are defined in `docs/ROADMAP/2025Q4.md`; supporting notes live in `docs/ROADMAP/notes/`.

All architectural decisions require an ADR (`docs/architecture/adr/`). If a new change lacks an ADR, create one before implementation.

# Architecture Overview

## Program Snapshot
- Program ID: codex-sdk
- Name: GPT-5 Codex SDK Toolkit
- Version: 0.1.0
- Updated: 2025-10-01T05:17:22Z
- Progress: 100% (health: green)

## Systems
| ID | Name | Purpose | ADR | RFC | Status | Dependencies | Roadmap Phase | Key Metrics |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| control-plane | Control Plane Automation | Govern architecture, documentation, and tasks from one manifest-driven pipeline. | ADR-0001 | RFC-0001 | active | — | m_q1 | quality_pct=95, cycle_hours=2, drift_tolerance_min=0 |
| doc-sync | Documentation Synchronizer | Assemble the architecture overview and ADR/RFC indices automatically from the manifest. | ADR-0002 | — | planned | control-plane | m_q1 | freshness_minutes=5, coverage_pct=100 |
| task-ops | Task Ops Governance | Generate decomposed tasks and keep them aligned with architecture intent. | ADR-0003 | — | planned | control-plane, doc-sync | m_q1 | update_latency_minutes=5, traceability_pct=100 |

## Traceability
| Task ID | Title | Status | Owner | System | Big Task | Epic | Phase |
| --- | --- | --- | --- | --- | --- | --- | --- |
| ARCH-001 | Manifest-driven sync engine | done | gpt-5-codex | control-plane | bigtask-arch-sync | sdk-foundation | m_q1 |
| ARCH-002 | Automated documentation synthesis | done | gpt-5-codex | doc-sync | bigtask-doc-ops | sdk-foundation | m_q1 |
| ARCH-003 | Task board governance | done | gpt-5-codex | task-ops | bigtask-arch-sync | sdk-foundation | m_q1 |
| TEST-001 | Pytest in agentcall verify | done | gpt-5-codex | control-plane | bigtask-test-pytest | sdk-foundation | m_q1 |
| OPS-001 | Doctor UX with tables and links | done | gpt-5-codex | control-plane | bigtask-doctor-ux | sdk-foundation | m_q1 |

## Documents
- ADR Index: docs/adr/index.md
- RFC Index: docs/rfc/index.md
- Manifest: architecture/manifest.yaml

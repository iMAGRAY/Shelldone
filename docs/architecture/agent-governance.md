# Agent Governance and Operations

## Overview
This specification defines how Shelldone manages third-party agent SDK adapters (OpenAI, Claude, Microsoft), secrets, lifecycle, observability, and developer tooling. It complements `docs/architecture/ai-integration.md` and UTIF-Σ.

## Adapter Lifecycle
1. **Registration**: adapters declared in `agents/manifest.json` with metadata (id, description, command, error_contains, version, signature).
2. **Installation**: `make agents-install` (TODO) bootstraps required runtimes (Python venv, npm ci) and verifies lock files.
3. **Smoke Verification**: `python3 scripts/agentd.py smoke` executed in `make verify-prepush`.
4. **Health Monitoring**: `shelldone-agentd` polls adapters via readiness handshake (`status: ready`). Failures emit `sigma.adapter.unhealthy` events.
5. **Upgrade/Rollback**:
   - Upgrades performed via `shelldone agent adapters update <id>` (future CLI).
   - Lock files signed (Ed25519). On failure, revert to previous signature snapshot.
   - `agents/REVOCATIONS.md` lists banned versions.

## Secrets Management
- All agent adapters require API keys (`OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `MICROSOFT_AGENT_API_KEY`).
- Keys are stored using Secret Manager service (`shelldone secrets`) with escrow to OS keyring + encrypted overlay.
- `scripts/agentd.py` loads secrets via `shelldone secrets export --adapter <id>`; environment injection done per process invocation.
- Secret rotation tracked in `state/secrets/ledger.json` with timestamp and owner.
- Policies enforce least privilege: adapters cannot read other secrets.

## Runtime Service (`shelldone-agentd`)
- Always-on daemon started by `make run-agentd` or autostart (TODO `shelldone agent serve`).
- Heartbeat to adapters; restarts on crash with exponential backoff.
- Health endpoints: `/healthz` (JSON), `/status` (adapter states), `/journal/event` ingestion.
- Crash recovery: writes last known status to `state/agentd/status.json`.

## UTIF-Σ Integration
- Σ-cap handshake caches adapter capabilities (`adapter://openai`, etc.).
- CLI `shelldone agent <cmd>` uses HTTP endpoints and handles fallback.
- Σ-pty proxy logs `sigma.proxy.disabled` when agentd unreachable (see ADR-0005).

## Observability
- Metrics exported via OTLP: `agent.adapter.ready{adapter=}` (gauge), `agent.exec.latency`, `agent.exec.errors`, `agent.journal.accepted`.
- Logs: `logs/agents.log` with structured entries (adapter, status, message).
- Alerts: adapter unhealthy >2 min, secret rotation overdue (>30 days), exec error rate >5%.

## Testing Strategy
- **Smoke**: runtime presence (already in `scripts/agentd.py`).
- **Contract**: add `tests/agentd_contract.rs` (TODO) verifying `/sigma/handshake`, `/ack/exec`, `/journal/event` across adapters (mocked).
- **Integration**: `make verify-full` spins up agentd + adapters in container, executes sample requests, records Continuum events.
- **Security**: secret scanner ensures API keys not checked into repo; `npm audit`/`pip audit` run in CI.

## Developer API
- Provide Rust crate `shelldone-agent-api` (TODO) exposing typed client for handshake/exec/journal.
- Provide JS CLI wrappers for automation (`shelldone agent`).
- Document sample workflows in `docs/recipes/agents.md` (pending).

## Roadmap
1. Implement autostart/service management for agentd.
2. Build secret manager CLI integration for adapters.
3. Add contract tests & CI containers for adapters.
4. Publish developer API crate and docs.
5. Integrate adapter health metrics into Prism dashboards.

## Related ADRs
- ADR-0001 UTIF-Σ Control Plane
- ADR-0002 ACK Kernel
- ADR-0003 Persona Engine
- ADR-0004 Capability Marketplace Hooks
- ADR-0005 Σ-pty Proxy Integration
- ADR-0006 Microsoft Agent SDK Adapter


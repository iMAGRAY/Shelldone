# Agent SDK Bridge Architecture

## Purpose
- Provide a deterministic, policy-governed bridge for OpenAI, Claude, and Microsoft Agent SDKs.
- Ensure agents receive identical control primitives (ACK, Continuum, Sigma guard) independent of vendor.
- Deliver a single governance surface (policy engine + telemetry) for all SDK integrations.

## Components
| Layer | Responsibility | Implementation |
|-------|----------------|----------------|
|Domain (`shelldone-agentd/src/domain/agents`)|Defines agent binding aggregate, value objects, domain events.|`AgentBinding`, `AgentDomainEvent`, `CapabilitySet` enforce invariants (non-empty capabilities, valid version/channel).|
|Application (`shelldone-agentd/src/app/agents`)|Co-ordinates repositories, policy checks, telemetry hooks.|`AgentBindingService` exposes `register/activate/deactivate/heartbeat/set_capabilities`.|
|Ports (`shelldone-agentd/src/ports/agents`)|Abstract storage of bindings.|`AgentBindingRepository` trait.|
|Adapters (`shelldone-agentd/src/adapters/agents`)|Reference implementation for dev/test.|`InMemoryAgentBindingRepository` for smoke tests; production adapters (Postgres, Redis) tracked under `AGNT-STORE`.|

## Binding Lifecycle
1. **Register** — CLI/API requests `provider`, `sdk_version`, `channel`, `capabilities`. Domain guarantees semver-like versions and non-empty capabilities before persisting.
2. **Activate** — Agent handshake succeeds → service transitions status to `Active`, records first heartbeat (for immediate liveness budget).
3. **Heartbeat** — SDK-side keepalive updates `last_heartbeat_at`. Missing heartbeat beyond SLA triggers `agent.guard`.
4. **Capability Update** — When SDK announces new tools, service mutates `CapabilitySet` atomically; duplicates rejected.
5. **Deactivate** — Admin or watchdog disables binding; heartbeat blocked until re-register.

## Microsoft Agent SDK Notes
- MS Agent SDK bridges use the same STDIO adapters as OpenAI/Claude with extra `capability.msauth` token refresh hook.
- Default capabilities: `agent.exec`, `fs.read`, `telemetry.push`, `persona.sync`.
- Policy template `policies/vendors/microsoft.rego` enforces Azure identity, outbound network guard, and clipboard restrictions.

## Governance & Observability
- Agent events fan into Continuum journal (`agent.binding`, `agent.heartbeat`, `agent.capabilities`).
- Prism metrics: `agent.binding.count{provider}`, `agent.heartbeat.age_ms`, `agent.capability.count` per provider.
- Policy engine cross-checks binding channel with environment: `preview` allowed only on dev personas.

## Roadmap
- `AGNT-PERSIST` — PostgreSQL adapter with optimistic locking.
- `AGNT-DISCOVERY` — publish binding registry via `/status` and Sigma notifications.
- `AGNT-TELEMETRY` — map vendor-specific metrics to Prism dashboards.

Reference docs: `docs/architecture/ai-integration.md`, `docs/architecture/pain-matrix.md` (#3, #24, #25), `docs/architecture/utif-sigma.md`.

# Ultimate Shelldone Terminal Blueprint

> Version: 2025-10-04 • Owner: imagray `<magraytlinov@gmail.com>` • Status: Draft → Ready for Implementation

## 1. System Overview
Shelldone evolves into an agent-native terminal platform that unifies user and AI automation flows. The architecture follows strict DDD boundaries with four primary bounded contexts:

1. **Sigma Guard** — terminal IO mediation, OSC/CSI sanitisation, policy enforcement, Continuum journaling.
2. **TermBridge** — orchestration of external terminal emulators with capability negotiation, consent management, clipboard/paste guard, and cwd sync.
3. **Agent Bridge** — vendor-neutral governance for OpenAI, Claude, and Microsoft Agent SDKs, exposing ACK primitives over MCP (WebSocket + gRPC).
4. **Continuum & Observability** — state snapshots, audit journal, OTLP telemetry, policy feedback loops, and persona-aware UX overlays.

Each context exposes hexagonal ports; adapters live under `shelldone-agentd/src/adapters/**` and `mux/src/**`. Infrastructure concerns (TLS, discovery, persistence) are isolated in dedicated adapters with configurable policies.

## 2. Pain-Point Coverage Matrix
The 30 pain points from `docs/architecture/pain-matrix.md` map to the following champion components:

| Pain IDs | Champion Component | Coverage Target | Notes |
|----------|--------------------|-----------------|-------|
| 1, 3, 27 | Sigma Guard + PasteGuardService | Guided overlay, bracketed paste default, Continuum event `paste.guard_triggered`. | UX guild owns overlays; Sigma proxy enforces bracketed paste downgrade for non-compliant terminals.
| 2, 10 | Sigma Guard Policy Engine | Default Rego bundle `policies/default.rego`; deny-list of escape sequences; OTLP counter `sigma.escape_blocked`. |
| 4, 11 | TermBridge + Mux roaming | QUIC keepalive with session resumption; handshake TTL ≤ 150 ms; fallback to std reconnection. |
| 5, 21 | TermBridge Capability Map | Graphics support enumerated per adapter; fallback to external preview. |
| 6, 7, 19, 28 | ClipboardBridgeService | Wayland/X11/Windows backends shipping; OSC 52 downgrade, metrics wired; next → tmux passthrough + persona policies. |
| 8 | char-props + TermBridge | Unicode version handshake; tests in `qa/unicode_matrix`. |
| 9, 18, 24 | Continuum Journal | Aggregates structured events; replay CLI, OTLP exports, policy audit. |
| 12 | tty-device adapter | Pane-level toggle for XON/XOFF, persisted per persona. |
| 13 | Env Activation Service | direnv/nvm wrappers via Agent Bridge `agent.env.activate`. |
| 14, 15 | UX Command Palette + Continuum Search | Shared fuzzy index across scrollback + Continuum snapshots. |
| 16 | Perf harness | k6 `utif_sigma.js`, GPU frame metrics CI gate. |
| 17, 30 | Docs automation | mkdocs site with persona onboarding, terminal compatibility cards. |
| 20 | shelldone-ssh guardrails | Policy hooks for SSH agent forwarding, secrets vault integration. |
| 22 | Theme Engine | Auto-contrast GA release, persona-specific presets. |
| 23 | Shell lint | Agent tool `agent.shellcheck` default in SDK bridge. |
| 25, 26, 29 | TermBridgeService | Tokenised bindings, focus validation, command-runner + CLI override conformance tests. |
| 31+ | Agent Marketplace | Persona-aware agent catalog with policy gating. |

## 3. Agent SDK Bridge (OpenAI, Claude, Microsoft)
- `AgentBinding` aggregate persists provider, channel, capability set, heartbeat SLA.
- STDIO adapters:
  - `agents/openai_stdio.rs`
  - `agents/claude_stdio.rs`
  - `agents/microsoft_stdio.rs` with `capability.msauth` handling, Azure token refresh hook, and persona-based scopes.
- Discovery: `/status` → `agent_bridge.bindings` section, surfaced in GUI toast (`AgentBridgeBadge`).
- MCP tools exposed to SDKs:
  - `agent.exec`, `agent.plan`, `agent.undo`, `agent.batch`
  - `termbridge.spawn/focus/send_text`
  - `continuum.snapshot/list/replay`
  - `policy.explain`, `telemetry.push`
- Governance:
  - Policies under `policies/vendors/{openai,claude,microsoft}.rego` enforce network zones, persona gating, clipboard restrictions.
  - Continuum events `agent.binding`, `agent.heartbeat`, `agent.capability_change` feed OTLP metrics.
  - Heartbeat SLA: default 15 s; missing heartbeat escalates to PasteGuard overlay and optional auto-disable.

## 4. Implementation Phases
1. **Sigma Guard Enhancements** — sanitize pipelines, downgrade fallback, journaling (AC::SEC-Σ::escape_block_rate::<=0.1%::2026-01-31::scope=/sigma-proxy).
2. **TermBridge Core** — complete IPC adapters, clipboard bridge, capability discovery (AC::TERM-1::adapter_pass_rate::>=95%::2026-02-15::matrix=7-terminals).
3. **Agent Bridge GA** — Microsoft SDK parity, heartbeat governance, marketplace integration (AC::AGNT-1::sdk_parity::OpenAI=Claude=Microsoft::2026-02-28::scope=/agentd).
4. **Continuum & Observability** — replay UI, OTLP dashboards, audit exports (AC::OBS-1::continuum_replay_latency_ms::<=120::2026-03-15::p95).
5. **Persona UX Polish** — guided overlays, consent flows, theme guard (AC::UX-1::persona_nps::>=70::2026-03-31::personas=Nova,Core,Flux).

Each phase ships with property/integration tests and CI gates (`python3 scripts/verify.py`, perf runner scripts, observability smoke jobs).

## 5. Tests & Quality Gates
- **Unit**: Sigma sanitiser fuzz (`sigma_proxy::tests::sanitize_*`), AgentBinding invariants, CapabilityMap property tests.
- **Integration**: `/termbridge/*`, `/status`, `/context/full`, Microsoft SDK heartbeat via stdio harness.
- **Contract**: Rego policy tests with golden decision tables.
- **Diff Coverage**: enforce ≥ 90% via `python3 scripts/verify.py` pipeline (diff-cover).
- **Performance**: k6 scenario `scripts/perf/termbridge_discovery.js` (p95 ≤ 80 ms for capability fetch) and `utif_sigma.js` (p95 ≤ 120 ms with sanitiser enabled).
- **Security**: OSV scanner + `cargo deny` gating on ship; credential leak detection (entropy + policy).

## 6. Observability & Telemetry
- Metrics namespace `shelldone.*` with key instruments:
  - `sigma.escape_blocked`, `sigma.session_count`
  - `termbridge.actions_total`, `termbridge.clipboard_bytes`
  - `agent.binding.count{provider}`, `agent.heartbeat.age_ms`
  - `continuum.snapshot.latency_ms`
- Distributed tracing via OTLP; Baggage includes persona, binding_id, terminal_id.
- Continuum journaling with Merkle roots stored per event for tamper detection.

## 7. Risks & Mitigations
| Risk | Impact | Mitigation |
|------|--------|------------|
| Terminal IPC drift | Loss of control functions | Adapter compatibility suite, nightly discovery checks, capability TTLs.
| Policy misconfiguration | Automation blocked or unsafe actions allowed | Rego unit tests, dry-run mode, policy explain API, staged rollout with canaries.
| Microsoft SDK auth refresh failure | Agent downtime | Build-in token refresh pipeline with retries, persona-specific fallback prompts, OTLP alert `agent.msauth.refresh_errors`.
| Clipboard data leakage | Security incident | Consent gating, per-persona policy, max payload thresholds, Continuum auditing with redaction.
| Performance regression from sanitiser | Latency increase | Perf mini-protocol gating, fallback to streaming mode when ratio > 1.2x baseline.

## 8. Work Packages & Ownership
- `SEC-Σ` — Owner: imagray (support: Security guild)
- `TERM-CORE`, `TERM-CLIP`, `TERM-UX` — Owner: imagray (support: TermBridge guild)
- `AGNT-GA`, `AGNT-MSAUTH`, `AGNT-MARKET` — Owner: imagray (support: AgentBridge guild)
- `OBS-CONT`, `OBS-OTLP` — Owner: imagray (support: Observability guild)
- `UX-PERSONA`, `UX-GUIDE` — Owner: imagray (support: UX guild)
- `DOC-PORTAL` — Owner: imagray (support: Docs guild)

Each package tracked in `todo.machine.md` with MVP checkpoints and Stage gates (`see docs/status.md`).

## 9. KPIs & Evaluation
- Latency budgets: Sigma sanitiser overhead ≤ 1.4× baseline; TermBridge spawn ≤ 250 ms p95.
- Reliability: Guided mode approval failure rate ≤ 0.5%.
- Adoption: ≥ 80% personas enable guided overlays; agent SDK usage evenly distributed (OpenAI/Claude/Microsoft variance ≤ 15%).
- Security: Zero critical CVEs outstanding; policy denial false-positive rate ≤ 3% (tracked via `policy.explain`).

## 10. Next Steps
1. Finalise policy bundles (`policies/vendors/*.rego`) with Microsoft-specific scopes.
2. Implement TermBridge clipboard bridge (Wayland/tmux/OSC 52) with metrics.
3. Harden TermBridge command runner (timeout taxonomy, `SHELLDONE_TERMBRIDGE_WEZTERM_CLI` override, focus orchestration) and exhaustively test not-supported/error paths.
3. Complete `/termbridge/*` integration tests and discovery harness.
4. Prepare developer quickstart, persona onboarding walkthrough, and AGNT marketplace UI mockups.
5. Schedule perf validation sessions (k6 + GPU frame capture) before GA.

---
Responsible engineer: `@imagray` (liaison: `@sigma`, `@termbridge`, `@agentd`). Update cadence: weekly sync → `reports/status.json`.

## 11. Alignment with RTF and MVP
- RTF Gate: см. `docs/architecture/rtf.md` — этот blueprint соответствует RTF-порогам (Σ-cap ≤5 ms p95, ACK overhead ≤3 ms p95, Continuum append ≤1 ms p95, TermBridge spawn ≤250 ms p95, TTI ≤20 мс). Все изменения в разделах 4–7 должны сопровождаться обновлением артефактов RTF.
- MVP Scope: см. `docs/ROADMAP/MVP.md` — Wave 1 покрывает минимально достаточные компоненты MVP. Любой выход за рамки MVP должен быть помечен флагами и не нарушать RTF.

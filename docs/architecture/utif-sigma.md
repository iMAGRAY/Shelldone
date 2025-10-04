# UTIF-Σ (Unified Terminal Intelligence Fabric)

## Purpose
UTIF-Σ defines the end-to-end control plane for Shelldone's agent-first terminal. It unifies terminal escape semantics, capability negotiation, agent command exchange, observability, and state recovery under deterministic and reversible contracts.

## High-Level Diagram
```
+-----------------+      +-------------------+      +------------------+
|  Applications   |<---->|   Σ-cap Handshake  |<---->|  Capability DB   |
+-----------------+      +-------------------+      +------------------+
         |                        |                           |
         v                        v                           |
+-----------------+      +-------------------+                |
|  Agents & UI    |<---->|    ACK Kernel     |----------------+
+-----------------+      +-------------------+                |
         |                        |                           |
         v                        v                           v
+-----------------+      +-------------------+      +------------------+
|   Σ-pty Proxy   |----->|  Σ-json Event Bus |----->| Prism Telemetry  |
+-----------------+      +-------------------+      +------------------+
         |                        |
         v                        v
+-----------------+      +-------------------+
| Continuum Store |<---->|   MCP Sidecar     |
+-----------------+      +-------------------+
```

## Channels
- **Σ-pty:** hardened PTY proxy applying the ESC/OSC sandbox and capability-aware fallbacks (default allowlist: CSI, OSC 0/2/4/8/52, OSC 133 markers, OSC 1337 graphics profile).
- **Σ-json:** duplex WebSocket or ZeroMQ channel that carries ACK packets, persona context, telemetry hints, and policy prompts.
- **Σ-cap:** capability handshake endpoint exchanging feature manifests between the terminal, shells, and remote hosts.

> Implementation status: `shelldone-agentd` exposes `/sigma/handshake`, `/ack/exec`, `/journal/event`, and `/healthz` over HTTP (default `127.0.0.1:17717`). Launch via `cargo run -p shelldone-agentd -- --state-dir state`.
> Быстрый запуск: `make run-agentd` (foreground). Для CLI проверки используйте `shelldone agent handshake --persona core`, `shelldone agent exec --cmd "echo hi"` или `shelldone agent journal --payload '{"note":"test"}'`.

### Σ-pty Integration (ADR-0005)
- Proxy lives inside `shelldone-mux-server` and intercepts all PTY reads/writes.
- Escape sandbox:
  - Allowlist: CSI, `OSC 0/2/4/8/52/133/1337`, `APC`, `DCS` (screen), `DCS tmux`.
  - Violations → `agent.guard` with policy enforcement (`security_level: hardened`).
- Capability downgrade:
  - On capability mismatch (tmux, ConPTY) proxy rewrites capabilities and emits `/journal/event` (`kind: "sigma.downgrade"`).
- Continuum hook: every command emit `kind: "pty.output"`, payload: bytes (truncated), spectral tags: `pty::<domain>`.
- Fallback: if agentd unavailable, proxy switches to legacy path, logs `sigma.proxy.disabled` once per session.
- Implementation tasks: `mux::domain`, `shelldone-client`, `portable-pty` integration, new k6 scenario `scripts/perf/utif_pty.js` (TODO).

### Σ-cap Handshake Schema
YAML profile negotiated at startup and on capability changes:
```yaml
version: 1
capabilities:
  keyboard: [kitty, legacy]
  graphics: [minimal, kitty, sixel]
  palette: true
  osc8: true
  osc52:
    write: whitelist
    read: confirm
  semantic_zones: osc133
  term_caps: [rgb, alt_screen_mirror]
  security_level: hardened
  clipboard_broker: integrated
  unicode_version: "15.1"
```
Failure to reach agreement triggers an explicit downgrade event on Σ-json and a visible toast in the UI/logs.

## ACK (Agent Command Kernel)
Eight primitive commands (extensible via macros) exposed to agents and humans:
1. `agent.plan` – submit declarative workflow graph with expected outcomes.
2. `agent.exec` – run a command inside a zone, attach OSC 133 markers, stream output.
3. `agent.form` – prompt for structured input (forms, confirmations, parameter edits).
4. `agent.undo` – revert using Continuum snapshot diff; SLA: ≤80 ms to apply.
5. `agent.guard` – request elevation or new capability; resolved through policy.
6. `agent.journal` – retrieve JSONL slices of the action log for reasoning.
7. `agent.inspect` – fetch context summary (`fs`, `git`, `proc`, `ports`).
8. `agent.connect` – open or bind to an MCP sidecar session (local or remote).

ACK packets contain:
```json
{
  "id": "uuid",
  "persona": "nova|core|flux",
  "command": "agent.exec",
  "args": {"cmd": "git status", "zone": "work"},
  "policy_ref": "policy://default#exec",
  "spectral_tag": "exec::work::low"
}
```

## Personas
- **Nova (Guided):** overlays, inline hints, automatic risk explanations.
- **Core (Adaptive):** hints only on policy or SLA violation.
- **Flux (Expert):** zero-noise mode; only telemetry toasts and blocking dialogs.
Personas are configured via `config/personas/*.yaml` and negotiated during handshake.

### Persona Guardrails (ADR-0003)
- Hints budget: Nova ≤6/min, Core ≤3/min, Flux 0/min (enforced by engine).
- Safety prompts: always require explicit ack; persona profiles determine text + severity.
- Telemetry: `persona.hints.count`, `persona.policy.prompt_latency`; thresholds validated in `make verify-full`.
- UX validation: SUS ≥85 (Nova), frustration rate <10%; experiments logged in `artifacts/ux/`.

## Continuum Workspace Graph
- Event-sourced log stored under `state/journal/*.jsonl` with spectral tags.
- Snapshots recorded every N events (`state/snapshots/{timestamp}.json`) with Merkle indices for fast diffing.
- Restore SLA: ≤150 ms to hydrate panes, agents, and persona state.
- Supports cross-device sync by shipping compressed diffs via MCP sidecar.

## MCP Sidecar
- Local daemon exposing gRPC/MCP endpoints (`fs.*`, `git.*`, `proc.*`, `codeactions.*`).
- Authenticated using mutual TLS or Noise; follows policy envelopes defined in Rego.
- Provides clipboard brokerage (Wayland/X11/WSL), remote execution scopes, and workspace snapshots.

## Security & Safety
- ESC/OSC sandbox denies unsafe sequences by default (OSC 52 read, OSC 1337 file upload, window title manipulations). Policy overrides require explicit consent.
- Rego policies track `security_level` (hardened, trusted, sandbox) and gating for each ACK command.
- Audit log entries are signed with Ed25519 and stored in `artifacts/telemetry/audit-*.jsonl`.
- Unsafe events emit `agent.guard` prompts with persona-specific UX.

## Performance Budgets
- Handshake (Σ-cap round-trip): ≤5 ms.
- ACK overhead (excluding command runtime): ≤3 ms.
- Event log append: ≤1 ms.
- Continuum restore: ≤150 ms.
- k6 open-model perf runs: warmup 15 s, 3×60 s, constant-rate pacer; thresholds (`p95 ≤15 ms`, `p99 ≤25 ms`, `error_rate <0.5%`).

## Observability
- Prism Telemetry exports OTLP metrics: `terminal.latency.input_to_render`, `terminal.undo.duration`, `agent.policy.denied.count`, `handshake.downgrade.count`.
- JSONL journal includes `cmd.start`, `cmd.stop`, exit codes, git diffs, artefacts.
- Alerts: SLA breach, policy denial spikes, snapshot restore failures.

## Testing Strategy
- **Property:** replay(log) equals restored state; unicode grapheme width invariants.
- **Contract:** SCH fallback (kitty → legacy), OSC sandbox enforcement, MCP auth negotiation.
- **Fuzz:** ESC/OSC parser fuzz against CVE catalogue; handshake fuzz for malformed profiles.
- **Perf:** k6 scenarios for `agent.exec` and Continuum restore, recorded in `artifacts/perf/`.

## Roadmap Alignment
Wave 1 (Foundations): implement Σ-pty proxy, Σ-cap handshake MVP, ACK core commands, Continuum baseline, perf smoke tests.
Wave 2 (Copilot Experience): persona overlays, adaptive hints, policy approvals, Prism dashboards.
Wave 3 (Hyper Reality): marketplace integration, collaborative agent sessions, distributed Continuum sync.

## References
- OSC 133 draft spec (FinalTerm lineage)
- Kitty Keyboard Protocol
- OSC 52 clipboard guidelines

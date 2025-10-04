# Persona Engine (Nova/Core/Flux)

## Goals
- Provide adaptive UX layers that minimise cognitive load for users and agents.
- Enforce hint budgets, guardrail prompts, and telemetry per persona.
- Maintain reversible configurations with instant switching and policy enforcement.

## Personas
| Persona | Mode      | Hints/min | Prompts                         | Telemetry                     |
|---------|-----------|-----------|---------------------------------|------------------------------|
| Nova    | Guided    | ≤ 6       | Auto show on SLA/policy breach | `persona.nova.hints`, SUS UX |
| Core    | Adaptive  | ≤ 3       | On policy breach only          | `persona.core.hints`         |
| Flux    | Expert    | 0         | Only blocking dialogs          | `persona.flux.alerts`        |

## Architecture
- Persona Engine service lives inside `shelldone-gui` and exposes:
  - `PersonaProfile` (immutable spec from YAML).
  - `HintBudget` (token bucket per persona, enforced every 10s).
  - `ApprovalFlow` (policy + ACK `agent.guard`).
- Integration points:
  1. Σ-cap handshake negotiates persona (default: `core`).
  2. UI overlays subscribe to persona context to render hints/coach marks.
  3. Telemetry emitted to Prism (`persona.hints.count`, `persona.prompt.latency`).
  4. CLI respects `SHELLDONE_AGENT_PERSONA`.

## YAML Format (`config/personas/*.yaml`)
```yaml
id: core
mode: adaptive
hints:
  enable: true
  max_per_min: 3
  triggers:
    - policy_pending
    - sla_violation
safety_prompts:
  auto_confirm: false
  require_ack: true
telemetry:
  collect: full
```
- Validation runs in `make verify-full`; invalid combinations fail with explicit errors.

## Guardrails
- `HintBudget` logs drops as `persona.hint.dropped`.
- `ApprovalFlow` merges persona policy with global Rego rules (e.g., `policy://default#exec`).
- UX metrics: SUS ≥ 85 (Nova), frustration < 10%; stored in `artifacts/ux/` with timestamp.

## Implementation Plan
1. Implement Persona Engine crate (`shelldone-persona`): parsing, budgets, guardrails.
2. Wire Σ-cap handshake → Persona Engine (fallback to `core`).
3. Add UI overlays and hint presenters (Nova). Guard for Core/Flux.
4. Telemetry emission via Prism (OTLP gauges/counters).
5. Add verify checks: YAML lint, telemetry sampling, unit tests (hint budget, guardrail).
6. Conduct UX studies: record SUS, publish results in `docs/recipes/ux/persona.md`.

## Rollback
- Env `SHELLDONE_PERSONA_ENGINE=0` disables engine, returning to legacy hints.
- Remove YAML files / revert handshake persona parameter.

## ADR Reference
- ADR-0003 (Persona Engine Nova/Core/Flux).


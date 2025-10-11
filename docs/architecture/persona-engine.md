# Persona Engine (Nova/Core/Flux + Experience Presets)

## Goals
- Минимизировать когнитивную нагрузку и обеспечить мгновенную адаптацию UX для людей и агентов.
- Предоставить режимы подсказок/guardrails, которые можно настраивать как персональные профили и как «уровни опыта».
- Обеспечить телеметрию и политики так, чтобы автоматика и люди принимали решения на фактах, а не на догадках.

## Personas
| Persona | Mode      | Hints/min | Prompts                         | Telemetry                     |
|---------|-----------|-----------|---------------------------------|------------------------------|
| Nova    | Guided    | ≤ 6       | Авто при SLA/политике и onboarding | `persona.nova.hints`, SUS UX |
| Core    | Adaptive  | ≤ 3       | Только при нарушении политики      | `persona.core.hints`         |
| Flux    | Expert    | 0         | Только блокирующие диалоги        | `persona.flux.alerts`        |

## Experience Presets
Персональные настройки объединяют personas + уровень опыта. Presets доступны в CLI/UI и по API (`persona.set`).

| Preset      | Persona | Подсказки | Guardrails | UX Default |
|-------------|---------|-----------|------------|------------|
| `beginner`  | Nova    | Максимум, walkthrough, inline coach | Подтверждение любых опасных команд, авто-remediation | Guided onboarding, подсветка клавиш |
| `ops`       | Core    | Контекстные подсказки, health HUD   | Подтверждение high-risk, автоматический журнал | Perf HUD включён, sigma guard overlay |
| `expert`    | Flux    | 0 шум, только telemetry             | Silent logging, блокирующий prompt только на policy breach | Минимальный UI, отключены tooltips |

Переключение preset обновляет:
- Hint budgets.
- Policy approval flow (auto-confirm, require_ack).
- UI overlays (coach marks, perf HUD, sigma warnings).
- Telemetry sampling (гранулярность/частота).

## Architecture
- Persona Engine сервис (Rust crate `shelldone-persona`, WIP) содержит:
  - `PersonaProfile` — immutable данные из YAML (`config/personas/*.yaml`).
  - `ExperiencePreset` — композиция persona + UX overrides.
  - `HintBudget` — token bucket (по умолчанию 10 сек тик) с метриками `shelldone.persona.hints` и `persona.hint.dropped{preset=}`.
  - `ApprovalFlow` — интеграция c Rego policy (`policy://default#exec`).
  - `PersonaTelemetry` — роутинг метрик в Prism (`persona.hints.count`, `persona.prompts.latency`).
- Σ-cap handshake получает persona/preset (параметры `persona`, `experience`).
- UI и MCP агенты подписываются на `persona.status` события (Σ-json).
- CLI `shelldone persona set <preset>` обновляет runtime + записывает в Continuum.

### Hint Budget State Machine

```
┌──────────┐  (tick)   ┌────────────┐  (emit hint)   ┌────────────┐
│  IDLE    │──────────▶│ REPLENISH  │───────────────▶│  ACTIVE    │
└──────────┘           └────────────┘                └────────────┘
      ▲                    │   │                           │
      │(budget=0)          │   │(budget <0)                │(cooldown)
      └────────────────────┘   └───────────────────────────┘
```

- `REPLENISH` запускается каждые `cooldown_ms` и пополняет токены до `max_per_min` (параметр из YAML).  
- При превышении бюджета событие не отправляется, логируется `persona.hint.dropped{preset,reason}`.  
- `ACTIVE` состояние публикует hints в Σ-json и Continuum (`persona.hints.delta`). Переход в `IDLE` происходит при отсутствии запросов в течение 30 с (configurable).

Все переходы записываются в Continuum (`kind=persona.state.transition`), чтобы агенты могли восстанавливать контекст после рестартов.

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
experience:
  presets:
    beginner:
      persona: nova
      overlays:
        - guided
        - keyboard_hints
      guardrail_level: strict
      hint_budget:
        max_per_min: 6
        cooldown_ms: 5000
    ops:
      persona: core
      overlays:
        - perf_hud
      guardrail_level: balanced
      hint_budget:
        max_per_min: 3
        cooldown_ms: 8000
    expert:
      persona: flux
      overlays: []
      guardrail_level: silent
      hint_budget:
        max_per_min: 0
        cooldown_ms: 0
```
Валидация происходит в `python3 scripts/verify.py --mode full`; несоответствия (например, preset без persona) приводят к ошибке сборки.

## Guardrails
- `HintBudget` логирует отбрасывание подсказок как `persona.hint.dropped{preset=}`.
- `ApprovalFlow` объединяет persona policy (YAML) и глобальные правила Rego. Подсказки “предложить remediation” отправляются через `agent.guard.suggest`.
- UX метрики: SUS ≥ 85 (Nova/beginner), frustration < 10% (Ops), time-to-task < 5s (Expert). Отчёты — `artifacts/ux/`.

## Onboarding Flow
1. First-run wizard задаёт 5 вопросов (опыт, цели, чувствительность данных).
2. Persona Engine предлагает preset (`beginner`, `ops`, `expert`).
3. Σ-cap handshake фиксирует выбор и публикует `persona.onboarding.complete`.
4. Пользователь/агент может изменить настройку позже (`persona.set`).

> **Текущее состояние:** автоматический wizard и preset API находятся в разработке (см. `docs/architecture/pain-matrix.md` — пункты #1, #3, #17). В релизе 2025Q4 используется временный переключатель `SHELLDONE_PERSONA`.

## API Surface
- Σ-json: `persona.status`, `persona.set`, `persona.list`, `persona.hints.delta`.
- CLI: `shelldone persona set`, `shelldone persona wizard`, `shelldone persona status`.
- MCP: `agent.guard.suggest`, `agent.inspect` возвращают активные preset/guards.

### Telemetry & Alerting

| Metric | Purpose | Alert |
|--------|---------|-------|
| `shelldone.persona.hints{preset}` | Количество выданных подсказок | >30 в час для `expert` → investigate (не должно быть нуля подсказок). |
| `persona.hint.dropped{preset}` | Потерянные подсказки из-за бюджета | >5 за 10 мин → увеличить бюджет либо пересмотреть UX. |
| `persona.onboarding.duration_ms` (roadmap) | Время прохождения мастера | p95 >120 с → UX review. |

Persona presets управляют TermBridge overlays: Beginner/Nova автоматически показывает confirmation overlay при подозрительном paste, Ops/Core — лёгкий inline hint, Expert/Flux — только статус-лог. Настройки описаны в YAML (`experience.overlays`) и влияют на PasteGuard политику (см. `docs/architecture/termbridge.md`).

Telemetry проксируется в Grafana дашборд “Persona Experience”. При ручном переключении пресета `shelldone persona set` публикует событие `persona.status` для всех подключенных агентов.

## Rollback
- `SHELLDONE_PERSONA_ENGINE=0` отключает персонализацию (fallback legacy hints).
- YAML preset можно удалить — handshake вернёт `core`/`ops` по умолчанию.

## ADR Reference
- ADR-0003 Persona Engine (дополнить разделом Experience Presets).

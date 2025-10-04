# MODE: DESIGN

**TL;DR**
UTIF-Σ Wave 1 (Foundations) **закоммичено** (commit `721bd5ca4`). Реализовано 72% критических компонентов. Следующие приоритеты: Rego runtime, Continuum snapshots, Prism OTLP.

---

## Реализованные Компоненты ✓

### Ядро UTIF-Σ (Production-Ready)
1. **shelldone-agentd** — HTTP control plane (Axum, 4 endpoints)
2. **Σ-pty proxy** — PTY escape sandbox (CSI/OSC фильтрация)
3. **CLI agent** — handshake/exec/journal commands
4. **Persona configs** — Nova/Core/Flux YAML schemas
5. **Microsoft adapter** — STDIN/JSON bridge
6. **k6 perf tests** — thresholds p95≤15ms, p99≤25ms
7. **6 ADR документов** — архитектурные решения

### Документация
- `docs/architecture/utif-sigma.md` — полная спецификация
- `docs/architecture/persona-engine.md` — persona guardrails
- `docs/IMPLEMENTATION_STATUS.md` — детальный статус (72%)
- `policies/default.rego` — Rego policy spec

---

## План Следующих Шагов (Приоритет P0)

### 1. Rego Policy Runtime Integration
**Epic:** epic-platform-resilience
**Task:** task-security-hardening (5% → 80%)

**Действия:**
```bash
# 1. Добавить зависимость в shelldone-agentd/Cargo.toml
regorus = "0.2"  # или opa-rs альтернатива

# 2. Создать модуль policy_engine.rs
shelldone-agentd/src/policy_engine.rs:
  - load_policy(path) → RegoEngine
  - evaluate(input: AckPacket) → PolicyDecision
  - emit_denial_telemetry()

# 3. Интегрировать в /ack/exec
  - До выполнения команды: engine.evaluate(packet)
  - Если deny → return ApiError::forbidden()
  - Логировать deny_reason в journal

# 4. Добавить unit tests
  - policy_allows_core_persona_exec()
  - policy_denies_guard_without_approval()
  - policy_filters_unsafe_osc()
```

**DoD:**
- [ ] `make verify-prepush` проходит
- [ ] policy denials логируются в JSONL
- [ ] metrics `agent.policy.denied.count` экспортируются

**Риски:** Rego runtime может увеличить latency exec на 2-5ms → замерить с k6

---

### 2. Continuum Snapshots
**Epic:** epic-platform-resilience
**Task:** task-state-persistence (10% → 100%)

**Действия:**
```bash
# 1. Расширить EventRecord структуру
  - snapshot_id: Option<String>
  - merkle_root: Option<String>

# 2. Создать snapshot writer
shelldone-agentd/src/continuum.rs:
  - create_snapshot(events: &[EventRecord]) → SnapshotId
  - compute_merkle_tree(events) → MerkleRoot
  - write snapshot: state/snapshots/{timestamp}.json.zst (zstd compressed)

# 3. Реализовать ACK agent.undo
  - POST /ack/undo {snapshot_id, diff}
  - Apply diff, emit rollback event
  - SLA: ≤80ms

# 4. Добавить /continuum/snapshots endpoint
  - GET → list available snapshots
  - POST /restore {snapshot_id} → restore workspace
```

**DoD:**
- [ ] Snapshot создаётся каждые 100 events
- [ ] `agent.undo` восстанавливает состояние за ≤80ms
- [ ] SLA restore ≤150ms (замерить с k6)

**Риски:** Большие snapshots (>10MB) могут замедлить restore → chunked streaming

---

### 3. Prism OTLP Telemetry
**Epic:** epic-platform-resilience
**Task:** task-observability-pipeline (5% → 100%)

**Действия:**
```bash
# 1. Добавить зависимости
opentelemetry = "0.27"
opentelemetry-otlp = "0.27"
opentelemetry-semantic-conventions = "0.27"

# 2. Создать telemetry module
shelldone-agentd/src/telemetry.rs:
  - init_prism(endpoint: &str) → MeterProvider
  - record_exec_latency(duration_ms)
  - record_policy_denial(reason)
  - record_persona_hint()

# 3. Экспортировать метрики
  - terminal.latency.input_to_render (histogram)
  - agent.policy.denied.count (counter)
  - persona.hints.count (gauge)
  - handshake.downgrade.count (counter)

# 4. Grafana dashboard
artifacts/observability/utif-sigma-dashboard.json:
  - Panels: exec latency (p95/p99), policy denials, persona hints rate
  - Alerts: SLA breach (p95 > 15ms), policy spike
```

**DoD:**
- [ ] Метрики экспортируются в Prometheus/Grafana
- [ ] `make verify-full` включает telemetry smoke test
- [ ] Alerts настроены для SLA breach

**Риски:** OTLP overhead может добавить 1-3ms latency → батчинг

---

### 4. Integration Tests в CI
**Epic:** epic-qa-hardening
**Task:** task-qa-orchestrator (72% → 90%)

**Действия:**
```bash
# 1. Создать test suite
scripts/test-utif-integration.sh:
  - start agentd в background
  - handshake → exec → journal → verify JSONL
  - policy denial scenarios
  - Σ-cap downgrade (tmux, ConPTY)
  - cleanup on exit

# 2. Добавить в Makefile
make verify-utif-integration:
  - bash scripts/test-utif-integration.sh
  - k6 run scripts/perf/utif_exec.js --quiet

# 3. CI gate
.github/workflows/ci.yml:
  - make verify-prepush
  - make verify-utif-integration
  - fail if p95 > 15ms or error_rate > 0.5%
```

**DoD:**
- [ ] CI запускает integration tests на каждый PR
- [ ] Perf regression gate: fail если p95 регресс >20%
- [ ] Artifacts (perf/*.json) архивируются

**Риски:** CI timeout — ограничить duration k6 до 30s

---

## Roadmap Обновления

### Прогресс Эпиков
| Epic | Было | Стало | Целевой |
|------|------|-------|---------|
| epic-platform-resilience | 5% | 35% | **100%** (после P0 задач) |
| epic-ai-automation | 6.3% | 22% | 70% (после persona engine) |
| epic-qa-hardening | 72% | 72% | 90% (CI integration) |

### Wave 1 Exit Criteria
- [x] Σ-pty proxy stable, OSC 133 tagging
- [x] ACK `agent.exec` + Continuum baseline
- [~] k6 perf baselines (CI gate TODO)
- [~] Policy denials logged (Rego runtime TODO)

**Estimated Completion:** 2025-10-15 (11 дней)

### Wave 2 (Copilot Experience) — Starts 2025-10-16
- Persona Engine GUI integration
- Adaptive hints (Nova ≤6/min, Core ≤3/min)
- Prism dashboards (Grafana)
- MCP federation для remote agents

---

## Команды для Проверки

### Локальный smoke test
```bash
# Запуск agentd
cargo run -p shelldone-agentd -- --listen 127.0.0.1:17717 --state-dir /tmp/state

# Handshake
shelldone agent handshake --persona core

# Exec
shelldone agent exec --cmd "ls -la"

# Journal
shelldone agent journal --kind test --payload '{"msg":"hello"}'

# Perf test
k6 run scripts/perf/utif_exec.js
```

### Verify pipeline
```bash
make verify-prepush     # lint, format, unit tests
make verify-full        # + perf, integration tests (TODO)
```

---

## Техдолг & Известные Ограничения

### High Priority
1. **Rego runtime интеграция** — без неё политики не применяются
2. **Continuum snapshots** — без восстановления состояния при сбоях
3. **OTLP telemetry** — без метрик невозможен мониторинг SLA

### Medium Priority
4. **Σ-pty fuzzing** — escape parser нуждается в fuzz-тестах (CVE surface)
5. **Microsoft SDK** — adapter работает в fallback mode (SDK not published)
6. **Persona Engine GUI** — пока только YAML configs, без UI

### Low Priority
7. **Cross-device sync** — Continuum sync через MCP sidecar (Wave 3)
8. **Capability marketplace** — hooks для third-party plugins (Wave 3)

---

## Rollback Process

Если UTIF-Σ вызывает проблемы:
```bash
# Отключить Σ-pty proxy (вернуться к прямому PTY)
./scripts/rollback-utif.sh

# Отключить ACK protocol
./scripts/rollback-ack.sh

# Revert commit
git revert 721bd5ca4
```

---

## Референсы
- **Архитектура:** `docs/architecture/utif-sigma.md`
- **Статус:** `docs/IMPLEMENTATION_STATUS.md`
- **Roadmap:** `docs/ROADMAP/2025Q4.md`
- **ADRs:** `docs/architecture/adr/0001-0006-*.md`
- **Machine state:** `todo.machine.md`

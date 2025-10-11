# Shelldone Observability and Operations

## Goals
- Provide full transparency into performance, errors, and agent behaviour.
- Detect regressions and degradations as early as possible.
- Enable post-incident investigations and reproducibility.

## Metrics

| Domain | Metric | Type | Source | Dashboard | Notes |
|--------|--------|------|--------|-----------|-------|
| Core | `terminal.latency.input_to_render` | histogram (ms) | GUI frame loop | Render Perf | p95 budget ≤20 ms, exported каждую секунду. |
| Core | `terminal.memory.rss` | gauge (MiB) | platform metrics | Infra Overview | См. perf budgets; предупреждение при росте >10% за 5 мин. |
| Mux | `mux.queue.depth` | gauge | mux scheduler | Operational | записывается для каждого домена/панели. |
| Agents | `agent.exec.latency` | histogram | ACK kernel | Agent Ops | p95 ≤150 ms; persona как label. |
| Agents | `agent.exec.errors` | counter | ACK kernel | Agent Ops | Совокупный error rate <1%. |
| Handshake | `handshake.downgrade.count` | counter | Σ-cap handshake | Agent Ops | label `reason`. |
| Security | `shelldone.policy.denials` | counter | PolicyEngine | Security | label `command`, `persona`. |
| Security | `shelldone.sigma.guard.events` | counter | Σ-pty proxy | Security | reason + direction. |
| TLS | `agent.tls.reloads` | counter | TLS watcher | Security | label `result=success|failure`, TTL 30 дней. |
| TLS | `agent.tls.reload_latency_ms` | histogram | TLS watcher | Security | SLA ≤5000 ms (AC::SEC-03). |
| TermBridge | `termbridge.actions` | counter | TermBridgeService | TermBridge Ops | labels `command` (`discover`, `spawn`, `send_text`, …), `terminal`, `outcome`. |
| TermBridge | `termbridge.latency_ms` | histogram | TermBridgeService | TermBridge Ops | end-to-end latency per команду; span включает вызовы CLI/IPC (например `wezterm cli spawn`). |
| TermBridge | `termbridge.errors` | counter | TermBridgeService | TermBridge Ops | labels `reason`, `terminal`. |
| TermBridge | `termbridge.consent.denied` | counter | HTTP layer | TermBridge Ops | labels `action`, `terminal`; инкремент при `requires_opt_in=true` без consent. |
| TermBridge | `termbridge.capabilities.discovered` | counter | Capability detector | TermBridge Ops | рост → новые терминалы/фичи (`source=bootstrap|mcp|manual`). |
| TermBridge | `termbridge.paste.guard_tripped` | counter | PasteGuardService | TermBridge Ops | label `heuristic` (`newline`, `zwsp`, `suspicious_unicode`). |
| TermBridge | `termbridge.clipboard.bytes` | histogram (bytes) | ClipboardBridgeService | TermBridge Ops | bucketed 1 KiB…512 KiB; labels `backend`, `channel`. |
| Persona | `shelldone.persona.hints` | counter | persona engine | UX | label `preset`, `hint_type`. |
| Experience Hub | `experience.approvals.pending` | gauge | Telemetry service | UX | Конвертирует pending approvals → gauge (единицы = approvals). |
| Experience Hub | `experience.approvals.source` | counter | Telemetry service | UX | label `source = http|http_fail|http_missing|local|none`. |
| Experience Hub | `experience.telemetry.ready` | counter | Telemetry service | UX | label `state=true|false`, рост показывает готовность снапшота. |
| Persona | `persona.hint.dropped` | counter | persona engine | UX | сигнал превышения бюджета подсказок. |
| Observability | `agent.tls.reload_errors` | counter | TLS watcher | Alert feed | label `error`. |
| Σ-json | `agent.status.broadcast_lag_ms` | histogram | Σ-json server | Agent Ops | следит за lag UI подписок. |
| UTIF-Σ | `/journal/event` throughput | derived | Continuum | Observability | см. `artifacts/perf/continuum/*.json`. |

Metrics push через OTLP HTTP (`--otlp-endpoint`). Collector по умолчанию — `http://localhost:4318`. Все сервисы используют одну resource метку `service.name=shelldone-agentd` + `service.version` с данными из Cargo.

### TermBridge Observability Hooks
- **Continuum Events:**
  - `termbridge.capability.update` — содержит diff Capability Map (labels `terminal`, `source`, `change=added|updated|removed`), используется `see docs/status.md` и UX подсказками.
  - `termbridge.action.{accepted,denied}` — action-level telemetry, связывается с policy решением (`policy_rule`).
  - `termbridge.paste.guard_triggered` — сообщает persona, heuristic, длину вставки.
  - `termbridge.focus{,.denied}` — фиксирует смену активного binding’а и отказы политики при попытке фокуса.
  - `termbridge.errors{reason}` — reason taxonomy (`timeout`, `io`, `not_supported`) отличает инфраструктурные проблемы от misconfiguration (например, отсутствующий CLI).
- **Discovery Registry:** `termbridge.capabilities.discovered` теперь питается registry-сервисом: каждое обновление публикует `registry_version`, `terminal`, `source` (`mcp`, `bootstrap`, `manual`). Метрика участвует в новых задачах `task-termbridge-discovery-registry` и `task-termbridge-discovery-mcp-sync`.
- **Experience Hub Persona:** GUI при синхронизации `TelemetrySnapshot` делает `POST /termbridge/discover`; `removed≠∅` понижает intent до `Recover`, наличие `requires_opt_in` в `added|updated` → intent `Focus`.
- **Clipboard Insight:** `ClipboardBridgeService` публикует `termbridge.clipboard.bytes` с backend/channel + статус (`success|error`) для мониторинга лимитов OSC 52 и fallback цепочек.
- **Span Annotations:** TermBridgeService оборачивает IPC вызовы в spans `termbridge.spawn`, `termbridge.send_text`, `termbridge.focus`. Атрибуты: `terminal`, `binding_id`, `payload_bytes` (ограничен ≤4).
- **Dashboards:** «TermBridge Ops» включает stacked area `termbridge.actions` по командам и heatmap latency. Alert: `termbridge.errors` > 5/min (severity Medium); `termbridge.paste.guard_tripped` spike (>20/min) уведомляет UX owner.
- **Discover Throughput Alert:** Grafana rule `termbridge.actions{command="discover"}` > 150/мин (5 мин) — уведомление P2 в `OPS::Agent`, создаёт follow-up `task-termbridge-discovery-overload` и требует оператора проверить backpressure конфиг.
- **Auth Telemetry:** при включённом `SHELLDONE_TERMBRIDGE_DISCOVERY_TOKEN` все запросы `/termbridge/discover` маркируются outcome `unauthorized` при отсутствии токена; GUI использует `SHELLDONE_AGENTD_DISCOVERY_TOKEN`, а при диагностическом `SHELLDONE_GUI_ALLOW_INSECURE_AGENTD=1` дополнительно пишется event `experience.telemetry: termbridge discover … exhausted retries`.
- **Verify Hooks:** `python3 scripts/verify.py --mode full` запускает `python3 scripts/tests/termbridge_matrix.py`, который дергает `shelldone-agentd --termbridge-export …` и валидирует Capability Map/документы. При необходимости можно передать `--emit-otlp`, чтобы убедиться, что `termbridge.*` метрики доходят до collector. Diff-coverage ≥90% контролируется `qa/baselines/coverage_termbridge.json`.
- **Capability Snapshot CLI:** `shelldone-agentd --termbridge-export artifacts/termbridge/capability-map.json` выполняет однократное обнаружение терминалов без запуска демона, возвращает JSON c `version`, `generated_at`, `discovery_ms`, `diff`, `totals`, `terminals[]`, `clipboard_backends[]`. Этот же CLI использует `scripts/tests/termbridge_matrix.py`, поэтому артефакт всегда готов для dashboards и CI.
- **Dashboards artefacts:** `scripts/tests/termbridge_matrix.py` зеркалирует актуальный snapshot в `dashboards/artefacts/termbridge/<os>.json`, предварительно убеждаясь, что `wezterm --version` проходит на каждом раннере и что mock OTLP collector содержит `termbridge.capability.update` datapoint для каждого терминала с `change ∈ {added, updated}`, ненулевым значением и непустым `source`. Drift ≥1 capability → workflow `termbridge-matrix` (macOS/Windows/Linux) падает, а в `dashboards/artefacts/termbridge/<os>-drift.json` сохраняется расхождение.

## Logs
- Structured JSON with standard levels (`trace`, `debug`, `info`, `warn`, `error`).
- Stored under `$XDG_STATE_HOME/shelldone/logs/`.
- Split into `core.log`, `agents.log`, `plugins.log`, and `security.log`.
- Size/time-based rotation with compression; optionally mirrored to journald.

## Tracing
- Wrap critical operations (render, exec, SSH, agent actions) in spans tagged `spectral_tag`.
- Persist span context in Continuum snapshots to reconstruct chains after failures.
- Provide `shelldone trace show` / `shelldone trace export` for offline analysis.

## Alerts and SLOs
- SLOs: input-to-render ≤ 20 ms at P99, crash-free sessions ≥ 99%, agent errors < 1%.
- Alert rules (Prometheus) route to Slack and Matrix.
- `python3 scripts/verify.py --mode ci` validates that the SLO configuration is in sync (lint step).

## Pipeline Integration
- `python3 scripts/verify.py --mode full` runs smoke tests with tracing enabled and checks JSON baselines generated by Prism.
- Performance artefacts (charts, JSON) live in `artifacts/perf/`.
- OpenMetrics snapshot пишется в `reports/perf/metrics.prom` (scrape: Prometheus `textfile` collector).
- Полный JSON-отчёт перф-проб — `reports/perf/summary.json` + `reports/status.json.perf.last_verify`.
- Статусы, roadmap и прогресс вручную фиксируются в `docs/status.md`, `docs/tasks.yaml` и `docs/architecture/manifest.md`.
- Policy regression tests assert denial/approval matrix for ACK commands.
- TLS-пайплайн публикует `reports/tls-status.json` с текущим fingerprint (roadmap) и предупреждением, если hot reload не подтверждён за SLA 5 секунд.
- `VERIFY_MODE=prepush TLS_SNAPSHOT=1 python3 scripts/verify.py` выполняет симуляцию ротации: генерирует временные PEM, ждёт reload, проверяет метрики (`agent.tls.reloads`). Отчёт складывается в `reports/verify/tls.json`.
- `scripts/verify.py --mode observability` проверяет, что перечисленные метрики присутствуют в OTLP export при запуске `cargo test -p shelldone-agentd --test e2e_mcp_grpc`; loopback perf-smoke добавляет экспорты для registry и clipboard (`task-termbridge-core-telemetry`).

## Dashboards & Alerting

| Dashboard | Purpose | Data Sources | Alert Hooks |
|-----------|---------|--------------|-------------|
| **Agent Ops** | SLA agent.exec, handshake, persona usage | `agent.exec.*`, `handshake.*`, `persona.*` | Slack `#shelldone-ops`, PagerDuty `OPS::Agent` |
| **Security** | TLS ротация, policy denials, sigma guard | `agent.tls.*`, `shelldone.policy.denials`, `shelldone.sigma.guard.events` | PagerDuty `SEC::Shelldone` |
| **TermBridge Ops** | Успехи команд termbridge, capability map | `termbridge.*`, `clipboard.transfer` | Slack `#shelldone-ops` |
| **Render Perf** | Frame latency, GPU utilization | `terminal.latency.*`, `terminal.memory.*` | Grafana annotations + `python3 scripts/verify.py --mode full` |
| **Continuum** | Journal throughput, snapshot restore | `/journal/event` throughput, `continuum.snapshots` | Email digest еженедельно |

Alert thresholds:
- Textfile ingest details: see `docs/observability/prometheus-textfile.md`. 
- `agent.tls.reload_errors` > 0 за 10 мин → incident severity Medium.  
- `agent.exec.latency` p95 > 150 ms в течение 5 мин → incident severity High.  
- `persona.hint.dropped` > 5 за 10 мин → UX owner review.
- `termbridge.actions{command="discover"}` > 150/мин (5 мин подряд) → PagerDuty `OPS::Agent` (severity P2); сигнал перегрузки discovery и потенциальной деградации GUI Sync.

## Plan
1. Publish an ADR covering collector choice, log format, and metrics storage.
2. Implement the `observability` crate and integrate it across the subsystems.
3. Ship a `shelldone obs view|export` CLI.
4. Wire observability checks into CI/CD (`python3 scripts/verify.py`, future GitHub Actions).
5. Document user workflows in `docs/recipes/observability.md`.

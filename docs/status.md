# Shelldone Status Board

> Последнее обновление: 2025‑10‑10 · Формат — **Готово / В процессе / Не готово**. Ссылки на тесты и артефакты указаны для проверки.

## 1. TermBridge
| Работа | Статус | Основание |
| --- | --- | --- |
| TermBridge CLI Matrix | **Готово** | `scripts/tests/termbridge_matrix.py`, workflow `termbridge-matrix`, `shelldone-agentd/tests/cli_termbridge.rs` |
| TermBridge Telemetry | **Готово** | `scripts/tests/check_otlp_payload.py`, `cargo test -p shelldone-agentd termbridge::telemetry` |
| Discovery Registry Snapshot | **Готово** | Snapshot `dashboards/artefacts/termbridge/<os>.json`, `cargo test -p shelldone-agentd termbridge::registry` |
| Core Orchestrator | **В процессе** | Основные тесты зелёные; остаются kitty/Windows адаптеры и ручная проверка CLI quoting |
| Discovery Core | **В процессе** | Параллельный discover → 1.3 мс на Linux (`scripts/tests/termbridge_matrix.py` от 2025‑10‑10); нет live MCP sync и backpressure |
| MCP Sync Watcher | **Не готово** | Реализация не начата |
| Backpressure Policy | **Не готово** | Требует дизайн-решения |

## 2. QA & Toolchain
| Работа | Статус | Основание |
| --- | --- | --- |
| make verify Orchestrator | **Готово** | `scripts/verify.py`: peak RSS/время в `reports/verify/summary.json`, baseline проверки зелёные |
| Forbidden Marker Baseline | **Готово** | Baseline актуален, проверяется перед release |
| Performance Probes | **В процессе** | Определены сценарии `scripts/perf_runner/specs.py`, требуется визуализация и CI |

## 3. Plugin Platform
| Работа | Статус | Комментарий |
| --- | --- | --- |
| Plugin SDK & Spec | **Не готово** | Нужен draft API и примеры |
| Reference Plugins & Themes | **Не готово** | Ждёт SDK |
| Marketplace/Webhooks | **Не готово** | Ожидает дизайн |
| Observability Reports | **Не готово** | Зависит от SDK |

## 4. AI Automation & Personas
| Работа | Статус | Комментарий |
| --- | --- | --- |
| MCP/gRPC Bridge | **В процессе** | Базовая интеграция есть, нет e2e тестов с Persona |
| Agent Policy Docs | **Не готово** | Требуется подготовить гайд |
| Persona Engine Evolution | **Не готово** | Бэклог, ждёт roadmap |
| Microsoft Agent SDK Adapter | **Не готово** | Ждёт спецификацию SDK |

## 5. Animation Engine / IDE / Resilience
Все крупные задачи в этих направлениях находятся в статусе **Не готово** — требуется проектирование и MVP (см. детали в `docs/tasks.yaml`).

### RFT журнал
- **TermBridge** — Nightly, лог `.github/workflows/termbridge_matrix.yml`.
- **QA** — Nightly `reports/verify.json`.
- **Plugin SDK** — Еженедельно `plugins/examples/README.md`.
- **Automation** — Еженедельно `reports/agents/mcp-demo.log`.
- **Animation** — Раз в две недели `artifacts/perf/animation/*.json`.

## 6. Как поддерживать актуальность
1. При завершении работы обновляйте `docs/tasks.yaml` и таблицы выше.
2. Прикладывайте ссылку на проходящие тесты/CI, иначе статус не считается «Готово».
3. Архитектурные изменения дополнительно фиксируйте в `docs/architecture/manifest.md`.
4. Перед сменой статуса прикладывайте последний RFT лог в `reports/roadmap/<scope>/`.

Полная разбивка и критерии по каждой задаче — в `docs/tasks.yaml`. Общее архитектурное видение см. `docs/architecture/manifest.md`.

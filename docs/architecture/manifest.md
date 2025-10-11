# Shelldone Architecture Manifest

> Версия документа: 2025-Q4 · Обновлено: 2025‑10‑07 · Контакты архитектуры: architecture@shelldone.dev

Этот manifest фиксирует текущее понимание архитектуры, целевые характеристики и критерии готовности. Он предназначен для инженеров, ведущих разработку Shelldone Terminal Platform, и заменяет прежние capsule-файлы.

## 1. Видение и целевые характеристики

### 1.1 Стратегия продукта
- **Shelldone Terminal Platform** — GPU-ускоренный терминал, мультиплексор и UX-платформа, открытая для плагинов и агентных сценариев.
- **Пользовательская ценность:** быстрый отклик, единый UX на Linux/macOS/Windows, расширяемость через SDK.

### 1.2 Нефункциональные цели (KPI)
- Аптайм ≥ 99.9 %
- Time-to-Interactive (TTI) ≤ 20 мс
- Переключение между сессиями ≤ 80 мс
- Ошибки терминального оркестратора ≤ 0.2 % вызовов
- Регрессии UI на релизе = 0 (все изменения проходят через QA-матрицу)

### 1.3 Цели квартала (см. OKR)
| Objective | KR | Метрика | Цель Q4 | Подсистема |
| --- | --- | --- | --- | --- |
| TermBridge устойчив | termbridge_matrix время | ≤12 мин (цикл macOS/Win/Linux) | ≤12 мин | TermBridge |
| QA автоматизирует регрессии | diff-cover доля | ≥95 % | ≥95 % | QA |
| Plugin SDK preview | Примеры проходят RFT | 2 примера | 2 | Plugin Platform |
| MCP e2e | Error rate | <1 % | <1 % | Automation |
| Animation буджет | Render loop p95 | ≤16 мс | ≤16 мс | Experience |

### 1.4 Политики качества
- Минимальный размер планируемой задачи — 5 story points.
- Каждый «готовый» элемент обязан иметь связанный тест/CI артефакт.
- Архитектурные и пользовательские документы обновляются в том же PR, что и код.

### 1.5 Ready-for-Test (RFT) ворота
| Scope | Обязательные проверки | Ответственный | Отчёт | Частота |
| --- | --- | --- | --- | --- |
| TermBridge | `cargo test` + `termbridge_matrix` | imagray (support: TermBridge Guild) | `.github/workflows/termbridge_matrix.yml` | Nightly + PR |
| QA Harness | `python3 scripts/verify.py --mode full`, `make review` | imagray (support: QA Guild) | `reports/verify.json` | Nightly + Release |
| Plugin SDK | `cargo check`, doctest примеров | imagray (support: Plugin Council) | `plugins/examples/README.md` | Weekly |
| MCP Automation | agent e2e playbook | imagray (support: Automation Guild) | `reports/agents/mcp-demo.log` | Weekly |
| Animation Engine | perf bench `animation_loop` | imagray (support: Experience Guild) | `artifacts/perf/animation/*.json` | Bi-weekly |

> Команда `make review` включает: проверку форматирования, `cargo clippy -- -D warnings`, `cargo nextest run`, тесты в режиме race (`cargo test -p shelldone-agentd --tests -- --nocapture`), e2e-тест `shelldone-agentd`, pytest, быстрый `VERIFY_MODE=fast scripts/verify.sh`, контроль дубликатов (`python3 scripts/check_duplication.py`), проверку сложности (`cargo clippy -- -W clippy::cyclomatic_complexity -W clippy::cognitive_complexity -D warnings`), валидацию контрактов (`python3 scripts/check_contracts.py`) и генерацию SBOM (`python3 scripts/generate_sbom.py`).

> Полные логи шагов `scripts/verify.py` автоматически сохраняются в `reports/logs/*.log`, а в консоль попадает только хвост (~4 КБ), чтобы удерживать пиковое потребление памяти посредника в пределах бюджета QA; при необходимости длину можно изменить через `VERIFY_TAIL_CHAR_LIMIT`.

#### 1.5.1 Ready-to-Field (RTF) ворота
Операционная готовность (безопасность/наблюдаемость/перф/откат) фиксируется в `docs/architecture/rtf.md` и проверяется через `make verify` → `make review` → `make ship`. Пороговые значения (p95, SCA, OTLP) и артефакты перечислены в RTF документе; все релизы обязаны прикладывать `reports/security.json` и perf артефакты.

### 1.6 Дорожная карта (мильстоны)
| Мильстон | Статус | Due | Основные критерии |
| --- | --- | --- | --- |
| QA Hardening | В процессе | 2025‑10‑31 | TermBridge матрица + forbidden marker baseline, быстрый verify |
| Plugin Platform MVP | В процессе | 2025‑11‑30 | SDK, примеры плагинов, документация |
| Platform MVP (сводное) | В процессе | 2025‑11‑30 | См. `docs/ROADMAP/MVP.md` — TermBridge/ACK/Σ-pty/Continuum/Obs/Plugin SDK |
| Animation Engine GA | Не готово | 2025‑12‑20 | Рендер ядро, визуализация и perf dashboards |

## 2. Ландшафт системы

### 2.1 Основные подсистемы
| Подсистема | Роль | Технологии | Код |
| --- | --- | --- | --- |
| `shelldone-agentd` | Контроль терминалов, TermBridge, OTLP export | Rust | `shelldone-agentd/src/**` |
| `shelldone-gui` | Desktop UI, визуальный UX | Rust (winit/egui) | `shelldone-gui/src/**` |
| TermBridge | Оркестрация внешних терминалов | Rust | `shelldone-agentd/src/app/termbridge/**` |
| Telemetry | OTLP и dashboards | Rust + Python | `shelldone-agentd/src/telemetry.rs`, `scripts/tests/check_otlp_payload.py` |
| QA Harness | verify, матрицы, perf | Shell + Python + Rust | `scripts/verify.py`, `scripts/tests/**`, `scripts/perf_runner/**` |

### 2.2 Интеграции
- **Terminals:** wezterm CLI, kitty remote, Windows Terminal.
- **Observability:** OTLP HTTP → Grafana/Tempo stack.
- **Agents/MCP:** gRPC мосты (в разработке) для AI/Persona.
- **Governance:** Roadmap и статус синхронизируются с `docs/ROADMAP/2025Q4.md`, `docs/status.md`, `docs/tasks.yaml`; RFT отчёты архивируются в `reports/`.

### 2.3 Деплой
- Desktop пакеты (brew, winget, deb/rpm).
- Distribs для daemon/CLI.
- Документация/SDK: https://shelldone.org.

## 3. Эпики и текущее состояние

| Эпик | Состояние | Область | Основные риски | Big tasks |
| --- | --- | --- | --- | --- |
| `epic-qa-hardening` | В процессе | CI/QA инфраструктура | Edge-платформы, рост времени verify | `task-qa-orchestrator`, `task-qa-perf-probes`, `task-termbridge-test-suite`, `task-qa-marker-baseline` |
| `epic-plugin-platform` | Не готово | SDK/IDE слой | Нет MVP, нечёткий API | `task-plugin-sdk`, `task-plugin-examples`, `task-marketplace-hooks`, `task-observability-reports` |
| `epic-ai-automation` | В процессе | MCP, persona, TermBridge | Неполное покрытие адаптеров | `task-mcp-bridge`, `task-termbridge-core`, `task-termbridge-discovery`, `task-persona-engine`, `task-agent-microsoft` |
| `epic-animation-engine` | Не готово | Render / визуализация | Нет ядра, нет perf-бюджета | `task-animation-core`, `task-animation-toolkit`, `task-observability-visual` |
| `epic-ide-dx` | Не готово | IDE-функциональность | Не начато | `task-ide-file-manager`, `task-state-sync-ui` |
| `epic-platform-resilience` | Не готово | Security, ops, state | Нет state persistence ядра | `task-state-persistence`, `task-security-hardening`, `task-release-hardening`, `task-utif-sigma-foundation`, `task-observability-pipeline` |

## 4. Big Tasks — критерии готовности

### 4.1 TermBridge
- **`task-termbridge-core`** — *В процессе*
  - Код: `shelldone-agentd/src/app/termbridge/**`
  - Тесты: `cargo test -p shelldone-agentd termbridge`, `cargo test -p shelldone-agentd --test e2e_ack`
  - Осталось: kitty/WT/konsole адаптеры, проверка Windows CLI quoting
- **`task-termbridge-discovery`** — *В процессе*
  - Тесты: `cargo test -p shelldone-agentd termbridge::discovery`
  - Пробелы: live MCP sync, backpressure
- **`task-termbridge-discovery-registry`** — *Готово*
  - Snapshot: `dashboards/artefacts/termbridge/<os>.json`
  - Тест: `cargo test -p shelldone-agentd termbridge::registry`
- **`task-termbridge-core-telemetry`** — *Готово*
  - OTLP-чекер: `scripts/tests/check_otlp_payload.py`
  - Тест: `cargo test -p shelldone-agentd termbridge::telemetry`
- **`task-termbridge-test-suite`** — *Готово*
  - Workflow: `.github/workflows/termbridge_matrix.yml`
  - Интеграционный тест: `shelldone-agentd/tests/cli_termbridge.rs`
- **`task-termbridge-discovery-mcp-sync`** — *Не готово*
- **`task-termbridge-backpressure`** — *Не готово*

### 4.2 QA
- **`task-qa-orchestrator`** — Готово (`scripts/verify.py`, базовые проверки)
- **`task-qa-marker-baseline`** — Готово (baseline поддерживается)
- **`task-qa-perf-probes`** — В процессе (нужна визуализация результатов `perf_runner`)

### 4.3 Plugin Platform / Animation / Resilience
- Все задачи — *Не готово*: требуют проектирования и MVP.

## 5. Нефункциональные бюджеты
- TermBridge discovery latency ≤ 200 мс (фиксируется тестом `scripts/tests/termbridge_matrix.py`; актуальный замер 1.3 мс на Linux, 2025‑10‑10).
- Telemetry export ≤ 100 мс.
- GUI FPS ≥ 60 в типовых сценариях (перф тесты TBD).
- Secrets — только в OS keychain, никаких plain-text `.env`.
- Каждый TermBridge action должен иметь OTLP event (чекером подтверждается).
- QA дифф-покрытие ≥95 %; превышение → немедленный блок merge.
- QA `make verify` публикует peak RSS/длительность для каждого шага (`reports/verify/summary.json` → budget check `resource-budget`).
- Plugin SDK примеры проходят doctest перед релизом.

## 6. Тестовая матрица
| Категория | Покрытие | Артефакты |
| --- | --- | --- |
| Unit (Rust) | `cargo test`, `cargo nextest` | CI planned, локально `make test` |
| Integration | `shelldone-agentd/tests/cli_termbridge.rs`, termbridge matrix | `.github/workflows/termbridge_matrix.yml` |
| Telemetry | OTLP payload checker | `scripts/tests/check_otlp_payload.py` |
| Perf | `scripts/perf_runner/specs.py` (k6), future dashboards | TBD |
| Docs | Manual review в рамках PR | README, `docs/**` |

## 7. Гавернанс обновлений
- Статусы поддерживаются в `docs/status.md` и `docs/tasks.yaml` (см. `docs/governance/status-updates.md`).
- Архитектурные изменения должны отражаться здесь и в профильных документах (`docs/architecture/*.md`).
- Любое «Готово» подкрепляется ссылками на тесты/CI.
- Перед закрытием мильстона прикладывайте RFT лог в `reports/roadmap/<milestone>/rft-<date>.md`.

## 8. Фокус на Q4 2025
1. Довести TermBridge MCP sync и backpressure.
2. Завершить performance probes (QA) и визуализацию.
3. Подготовить Plugin SDK draft и пример.

> Вопросы/правки — через issue или PR с обновлением этого документа.

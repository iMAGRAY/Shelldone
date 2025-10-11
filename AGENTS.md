# AGENTS.md — Orientation for Shelldone AI Agents

## Непреложные правила
- Единственный владелец и ответственный за все изменения: **imagray** `<magraytlinov@gmail.com>`.
- Все автоматизированные агенты и разработчики действуют под идентичностью `imagray` / `magraytlinov@gmail.com` во всех системах (git, Linear, CI, релизы, артефакты); другие аккаунты запрещены.

## Where to Find Knowledge
- Architecture: `docs/architecture/README.md`
- Roadmap: `docs/ROADMAP/2025Q4.md` + `docs/ROADMAP/MVP.md`
- Observability: `docs/architecture/observability.md`
- Security: `docs/architecture/security-and-secrets.md`, `docs/security/runbook.md`
- Performance budgets: `docs/architecture/perf-budget.md`

## Quality & Pipelines
- Entry point: `make verify` (`VERIFY_MODE=fast|prepush|full|ci`).
- Shortcuts: `make verify-prepush`, `make verify-ci`.
- Artifacts: `artifacts/verify/`, `artifacts/perf/`.
- Baselines: `qa/baselines/*` (update via `python3 scripts/verify.py --update-*-baseline`).
- Roadmap health: `make roadmap` (supports `JSON=1`, `STRICT=0`).

## Operations
### Pre-flight
- `git status --short` чистый перед пайплайнами.
- `python3 scripts/project_health_check.py --json` — быстрые проверки конфигурации/дрифта.

### Daily Loop
1. `make status` — синхронизировать прогресс/борд.
2. `VERIFY_MODE=prepush make verify` (или `make verify-prepush`).
3. Дифф‑покрытие ≥90% изменённых строк.
4. Перед merge/release: `make review`, затем `make ship`.

### Quick Reference
- `make verify` — fmt/lint/tests/SCA/perf/docs → `reports/verify.json`.
- `make review` — дифф‑проверки.
- `make ship` — релизный гейт + SBOM/SCA/locks.
- `make roadmap` — сверка заявленного/фактического прогресса.
- `python3 scripts/status.py` — агрегация статуса (`reports/status.json`).

## Operating Procedure for GPT-5 Codex
1. Проверить `git status --short`.
2. Согласовать план с `todo.machine.md` и `docs/ROADMAP/2025Q4.md`.
3. Внести изменения по архитектурным правилам (state/security/observability/release).
4. Прогнать: `VERIFY_MODE=fast make verify`; перед пушем — `VERIFY_MODE=prepush make verify`; перед релизом — `VERIFY_MODE=ci make verify`.
5. Обновить Roadmap/статус и соответствующую документацию.

Всегда обновляйте релевантные документы в одном PR с кодом.

## Linear Governance — Flagship+++
- Работа без Linear запрещена: каждый коммит, PR, релиз и эксперимент привязан к `Type::*` issue с корректными `Area::*`, `Workflow::*`, `Priority::P*`, ссылкой на проект `Shelldone Terminal Platform — Q4 2025` или актуальный successor.
- Канбан: единственный источник правды — доска Team `Eawea` ([board](https://linear.app/eawea/team/EAW/board) по `Workflow::*`), фильтры `project: Shelldone Terminal Platform — Q4 2025`, `labels includes Type::*`; WIP-лимиты `In Progress` ≤4, `Review` ≤3, нарушение → `Priority::P0` blocker.
- Перед стартом задачи исполнитель переводит issue в `Workflow::In Progress`, фиксирует ожидаемый результат в разделе Acceptance (Given-When-Then) и добавляет контрольные метрики (perf, diff-cover, error budget) с источниками (`reports/*`, `docs/status.md`).
- Каждое изменение состояния (код, конфиг, релиз, инцидент) немедленно отражается в Linear: чеклисты обновлены, ссылки на PR (`git remote`) и команды (`make review`, `make guard`, `make ship`) добавлены в Timeline, логи прикреплены как attachments или ссылки на `reports/logs/*`.
- До 11:00 UTC каждый рабочий день владелец проекта публикует project update: статус целей, расход error budget, blockers; без апдейта разработка считается замороженной.
- Перед merge запрещено закрывать issue без верификации: `make review` + `VERIFY_MODE=prepush make verify` + артефакты в issue; релизы требуют `make ship` и запись smoke-тестов, иначе issue переводится в `Workflow::Blocked`.
- Любой blocker/регрессия оформляется отдельной issue с `Priority::P0/P1`, ASCII-планом RCA, ссылкой на журналы и целевым временем разблокировки; до закрытия blocker все зависимые задачи маркируются `Workflow::Blocked (Waiting)`.
- Создание и triage задач выполняется исключительно через `make create_issue` (обёртка над `scripts/tools/create_linear_issue.py`); ручное создание через UI допустимо только при аварийном режиме с последующим RCA в Linear-issue.
- `LINEAR_API_KEY` хранится в секрет-хранилище (1Password: `Linear API Token — imagray`) и экспортируется в окружение перед запуском `make create_issue`; запись токена в репозиторий или логи запрещена.
- `todo.machine.md` — проекция Linear: каждая запись ссылается на конкретный issue (`LIN-XXX`), редактирование файла разрешено только после обновления статуса и чеклистов в Linear.
- Доска Team `Eawea` поддерживается вживую: обязательные колонки `Backlog → Ready → In Progress → Review → QA → Done`, дополнительная `Blocked (Waiting)` для ожиданий; SLA: `In Progress` ≤48h, `Review` ≤24h, нарушение фиксируется отдельным blocker-issue.
- Ежедневно до 10:00 UTC выполняется `make status` и визуальная сверка канбана; расхождения между Linear и локальными артефактами трактуются как блокер до устранения.

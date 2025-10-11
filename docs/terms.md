# Термины Shelldone

- **Готово / В процессе / Не готово** — статусы задач в `docs/status.md` и `docs/tasks.yaml`. Без процентов.
- **Guild** — профильная группа поддержки (например, TermBridge Guild, QA Guild); стратегический владелец всех направлений — imagray `<magraytlinov@gmail.com>`.
- **Snapshot** — итоговый отчёт (status + tasks + manifest) перед релизом.
- **Mock OTLP collector** — скрипт `scripts/tests/mock_otlp_collector.py`, применяемый в CI и локально для проверки телеметрии.
- **Matrix workflow** — `.github/workflows/termbridge_matrix.yml`, валидация TermBridge на macOS/Windows/Linux.
- **Perf runner** — Python интерфейс `scripts/perf_runner`, управляющий k6 сценариями.
- **MCP bridge** — модуль `shelldone-agentd` для интеграции с MCP/Agents.
- **Continuum** — журнал пользовательских/агентных действий (см. `docs/architecture/utif-sigma.md`).
- **Baseline** — файлы в `qa/baselines/` (forbidden markers, clippy). Обновляем только осознанно.
- **Manifest** — `docs/architecture/manifest.md`, центральный документ архитектуры.

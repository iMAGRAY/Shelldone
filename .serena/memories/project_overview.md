## Назначение
Shelldone — GPU-ускоренный кроссплатформенный терминал/мультиплексор (форк WezTerm) с расширяемостью через SDK и агентные сценарии.

## Техстек
- Rust 2021 workspace (основные бинарники: `shelldone-gui`, `shelldone-mux-server`, `shelldone-agentd`, вспомогательные крейты `strip-ansi-escapes`, `termwiz`, `mux`, `term`, и др.).
- Python 3 для оркестрации QA (`scripts/verify.py`, `perf_runner`, e2e тесты).
- Lua (конфигурация/SDK), небольшие C/C++ модули в `async_ossl`, `pty`.
- MkDocs для документации.

## Структура
- Корень содержит много Rust-крейтов (`shelldone-*`, `termwiz`, `mux`, `term`, др.).
- `docs/` — архитектура, roadmap, политики.
- `scripts/` — QA/инфра автоматизация (`verify.sh`, `review.sh`, `perf_runner`).
- `qa/baselines/` — контроль маркеров и clippy.
- `reports/`, `artifacts/` — артефакты QA/перф.
- `tests/`, `shelldone-agentd/tests/` — интеграционные и e2e тесты.
- `Makefile` — единая точка входа в пайплайны (`verify`, `review`, `ship`, perf-команды).

## Доп. документы
- Архитектурный manifest: `docs/architecture/manifest.md`.
- Статус работ: `docs/status.md`.
- Бэклог/эпики: `docs/tasks.yaml`, `todo.machine.md`.
- Политики безопасности/наблюдаемости: `docs/architecture/security-and-secrets.md`, `docs/architecture/observability.md`.

## Ключевые цели Q4 2025
Стабилизировать TermBridge, усилить QA и перф бюджеты, подготовить Plugin SDK.
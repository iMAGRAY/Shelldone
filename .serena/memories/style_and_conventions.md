## Стиль и конвенции
- Rust edition 2021, обязательный `cargo +nightly fmt`; форматирование проверяется `make verify`/`make review`.
- Линтинг: `cargo clippy --workspace --all-targets -- -D warnings`; новые предупреждения запрещены, baseline в `qa/baselines/clippy.json`.
- Тестирование: `cargo nextest run`, `cargo test` для отдельных крейтов, e2e в `shelldone-agentd/tests` и `scripts/tests/*`.
- Требуются тесты и обновление документации при каждом изменении поведения.
- Политики качества из `docs/architecture/manifest.md`: DI/DDD подход, все готовые элементы сопровождаются тестами/CI; архитектурные и пользовательские доки обновляются вместе с кодом.
- Приоритет на GPU-перф, устойчивость, наблюдаемость; соблюдать перф бюджеты (`perf_runner`, артефакты в `artifacts/perf`).
- Строгие правила по побочным эффектам (claim-first, transactional outbox, идемпотентность); описывать состояния и деградации.
- Коммиты в повелительном наклонении, ветки `feature/*`, `fix/*`, `docs/*`.
- Использовать feature flags/канарейки для рискованных изменений.
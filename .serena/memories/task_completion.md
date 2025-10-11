## Завершение задачи
1. Убедиться, что код покрыт тестами и отражает архитектурные политики; обновить `docs/` (архитектура/статус/roadmap) при любых изменениях поведения.
2. Запустить `make review` (включает fmt-check, clippy, nextest, race, dup, complexity, contracts, sbom). При необходимости — `VERIFY_MODE=prepush make verify`.
3. Проверить перф бюджет/артефакты (`python3 -m perf_runner run ...`, обновить `artifacts/perf` при изменениях hot-path).
4. Обновить `docs/status.md`, `docs/tasks.yaml`, `todo.machine.md` (или связанные дорожные документы) и приложить нужные логи в `reports/`.
5. Убедиться, что SBOM и security отчёты (`reports/security.json`) актуальны; перед релизом пройти `make ship`.
6. Подготовить PR с тестовыми/перф артефактами, ссылками на CI и кратким описанием изменений.
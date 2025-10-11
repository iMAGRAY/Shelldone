# Shelldone Contributor Handbook

> Последнее обновление: 2025‑10‑10

Этот документ дополняет [`CONTRIBUTING.md`](../../CONTRIBUTING.md) рабочими инструкциями: как разворачивать окружение, какие команды запускать, куда смотреть в документации.

## 1. Окружение

```bash
git clone git@github.com:imagray/Shelldone.git
cd Shelldone
./get-deps                 # нативные зависимости (опционально)
python3 -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
rustup toolchain install stable
cargo install cargo-nextest cargo-deny --locked
```

- Node/TypeScript и Go ставим по потребности (`npm install`, `go install`).
- macOS/Windows: установите `psutil` (`pip install psutil`) — без него `scripts/verify.py` не снимет peak RSS, и resource budget останется пустым.
- Для управления выводом QA используйте `VERIFY_TAIL_CHAR_LIMIT` (число в символах, по умолчанию 4000).
- Весь tooling работает из стандартного каталога проекта — никаких капсул `.agentcontrol`.

## 2. Повседневные команды

| Задача | Команда | Комментарий |
| --- | --- | --- |
| Быстрый линт/тест | `python3 scripts/verify.py --mode fast` | Проверяет forbidden markers, Rust fmt/clippy (ограниченный набор), базовые тесты |
| Полный прогон | `python3 scripts/verify.py --mode prepush` | Рекомендуем перед PR; при необходимости добавляйте свои цели |
| TermBridge матрица | `python3 scripts/tests/termbridge_matrix.py --emit-otlp --otlp-endpoint http://127.0.0.1:4318` + mock collector | См. README в `scripts/tests/` |
| Telemetry smoke | `python3 scripts/tests/check_otlp_payload.py --payload <file> --snapshot artifacts/termbridge/capability-map.json` | Проверяет, что OTLP метрики непротиворечивы |
| Agentd smoke | `python3 scripts/agentd.py smoke` | Проверяет базовые сценарии работы daemon и адаптеров |

Полный список задач и статусов находится в `docs/tasks.yaml`; архитектура — `docs/architecture/manifest.md`.

## 3. Workflow разработки
1. Выберите задачу в `docs/tasks.yaml` или создайте issue.
2. Создайте ветку `feature/<topic>` от `main`.
3. Пишите код, сопровождайте тестами/документацией.
4. Перед PR выполните `python3 scripts/verify.py --mode prepush` и релевантные целевые команды (см. таблицу выше).
5. Обновите `docs/status.md`/`docs/tasks.yaml`/`docs/architecture/manifest.md`, если изменился статус работы.
6. Откройте PR, укажите пройденные тесты и ссылки на документацию.

## 4. Тестовая матрица
| Категория | Команда | Примечания |
| --- | --- | --- |
| Rust unit/integration | `cargo test` / `cargo nextest` | Используется внутри `scripts/verify.py` |
| TermBridge CLI | `shelldone-agentd/tests/cli_termbridge.rs` (через `cargo test -p shelldone-agentd --test cli_termbridge`) | Проверяет CLI и capability export |
| QA resource budget | `python3 scripts/verify.py --mode fast` | `reports/verify/summary.json` содержит `resources.top_peak_kb`/`top_duration_sec`; пороги управляются `VERIFY_PEAK_KB_LIMIT[_NAME]`, `VERIFY_DURATION_LIMIT_SEC[_NAME]`; полные логи шагов лежат в `reports/logs/*.log` |
| Telemetry | `scripts/tests/check_otlp_payload.py` | Работает вместе с mock OTLP collector |
| Perf | `scripts/perf_runner/specs.py` | Пока вручную; для TermBridge discovery можно задать `SHELLDONE_PERF_TERMBRIDGE_ENDPOINT`/`…_TOKEN`, результаты прикладывайте к PR |
| Docs | Ревьюйте обновления в `docs/` в том же PR | Никаких auto-docs |

## 5. Кодстайл
- Минимизируйте `panic!`/`unwrap!` в библиотечном коде; отдавайте предпочтение `Result`.
- Используйте `tracing` с именованными полями.
- Обрабатывайте пользовательский ввод безопасно (никаких непроверенных путей).
- Асинхронные задачи — на стандартном движке (`tokio` здесь, `smol` в утилитах).
- Пишите rustdoc и комментарии к нетривиальным блокам.

## 6. Review и merge
- Каждый PR требует ревью мейнтейнера (см. CODEOWNERS).
- Всегда указывайте, какие тесты вы запускали.
- Мы используем squash merge; для user-facing изменений обновляйте документацию/roadmap.

## 7. Коммуникация
- Questions & proposals: GitHub Discussions (`imagray/Shelldone`).
- Чат: Matrix `#shelldone:matrix.org`.
- Security: [team@shelldone.dev](mailto:team@shelldone.dev).

## 8. Полезные ссылки
- Состояние работ — `docs/status.md`
- Архитектура — `docs/architecture/manifest.md`
- Правила обновления статусов — `docs/governance/status-updates.md`
- Roadmap — `docs/ROADMAP/`

Нашли несовпадение между кодом и документацией? Обязательно откройте issue или PR с исправлением. Спасибо за вклад!

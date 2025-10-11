# State Backup and Restore Workflow

Shelldone поддерживает быстрый rollback среды разработки через связку CLI `shelldone` и UI State Sync.

## Сохранение снепшотов
- Убедитесь, что установлен CLI `shelldone` (или альтернативный `SHELLDONE_STATE_CLI`).
- Выполните `shelldone state save --snapshot <label>` в корне рабочей капсулы.
- Автоматические бэкапы формируются агентами (см. `docs/architecture/state-and-storage.md`).

## Восстановление через UI
- Вызовите Experience Hub (`Ctrl+Shift+E` / `Cmd+Shift+E`).
- В карточке **State Sync** используйте:
  - `Ctrl+Shift+R` — восстановление последнего снепшота (CLI запускается в фоне).
  - `Ctrl+Shift+O` — открытие каталога снепшотов в проводнике.
  - `Ctrl+Shift+C` — копирование пути в буфер.
- Shelldone показывает тосты об успехе/ошибке. Если CLI недоступен, появляется уведомление с подсказкой по установке.

## Восстановление через CLI
- Запустите `shelldone state restore --snapshot <id>`.
- Можно указать каталог вручную через `SHELLDONE_STATE_SNAPSHOT_PATH`.

## Троссирование и отладка
- `/status` и Experience Hub отображают текущие снапшоты.
- При ошибках CLI проверяйте PATH и переменные `SHELLDONE_STATE_CLI`, `SHELLDONE_CLI`.

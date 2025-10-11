# Shelldone IDE Workflows (Draft)

Reserved for IDE-grade scenarios: file manager flows, LSP/DAP usage, and
automation patterns. Populate as part of `epic-ide-dx`.

## State Sync UI (preview)
- Откройте Experience Hub (`Cmd+Shift+E` или `Ctrl+Shift+E`) — вверху появится
  карточка **State Sync**.
- Верхний callout показывает последний снапшот с датой, размером и ссылкой на
  путь (`state/snapshots/*.json.zst`).
- Основная карточка перечисляет до 6 свежих снапшотов, помечая теги (`auto`,
  `manual`, `protected`) и возраст.
- Сочетания клавиш `Ctrl+Shift+R`, `Ctrl+Shift+O`, `Ctrl+Shift+C` выполняют
  восстановление, открытие каталога и копирование пути соответственно; после
  каждого действия всплывает тост с результатом (успех/ошибка, подсказка при
  недоступном CLI).
- Прогресс-бар справа отражает интенсивность автоматических сохранений: зелёная
  зона означает, что инкрементальные бэкапы выполняются своевременно.
- Нет снапшотов? Панель предложит команды `shelldone state save` и
  `shelldone sync push` для быстрого старта.

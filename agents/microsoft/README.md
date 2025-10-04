# Shelldone ↔ Microsoft Agent SDK Adapter

Этот адаптер обеспечивает соединение между Shelldone (`shelldone-agentd`)
и Microsoft Agent SDK.

## Требования
- Node.js ≥ 18
- Установленный пакет `@microsoft/agents-sdk`
- Переменная окружения `MICROSOFT_AGENT_API_KEY`

## Установка
```bash
cd agents/microsoft
npm ci
```

## Запуск (ручной)
```bash
MICROSOFT_AGENT_API_KEY=... node bridge.mjs
```

Процесс слушает команды в формате JSON через STDIN. Пример сообщения:
```json
{"type":"run","session":"demo","input":"list active processes"}
```

Для корректной работы `shelldone-agentd` должен запускать этот адаптер согласно
записям в `agents/manifest.json`.

Если пакет `@microsoft/agents-sdk` или переменная окружения отсутствуют,
скрипт вернёт структурированную ошибку (используется в smoke-тестах).

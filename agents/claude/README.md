# Shelldone ↔ Claude Agent SDK

Адаптер подключает [Claude SDK](https://docs.anthropic.com/en/docs/claude-agent-sdk)
к Shelldone. Он оборачивает `@anthropic-ai/sdk` в простой STDIO-протокол и
сохраняет краткосрочную память сессии в процессе.

## Установка

```bash
cd agents/claude
npm ci
export ANTHROPIC_API_KEY=sk-ant-...
node bridge.mjs
```

Команды отправляются построчно (JSON). Примеры:

```bash
echo '{"type":"run","input":"Привет!"}' | node bridge.mjs
echo '{"type":"shutdown"}' | node bridge.mjs
```

## Файлы
- `package.json` / `package-lock.json` — зафиксированные зависимости.
- `bridge.mjs` — реализация адаптера.
- `README.md` — этот файл.

## Протокол
- `{"type":"run", "input":"...", "session":"id"}` — выполнить запрос.
- `{"type":"shutdown"}` — завершить процесс.

Ответы:

```json
{"status":"ready","model":"claude-3-5-sonnet-latest"}
{"status":"ok","output":"...","session":"id"}
{"status":"error","error":"описание"}
```

## Обновление зависимостей
1. `npm update @anthropic-ai/sdk`
2. `npm install --package-lock-only`
3. `npm ci && make verify`

## Примечания
- Адаптер хранит историю беседы в памяти процесса; переключатель на Redis
  или другую БД планируется добавить после интеграции с `shelldone-agentd`.
- В случае отсутствия `ANTHROPIC_API_KEY` возвращается структурированная ошибка.

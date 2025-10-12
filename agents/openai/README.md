# Shelldone ↔ OpenAI Agents SDK

Этот адаптер подключает [OpenAI Agents SDK](https://github.com/openai/openai-agents-python)
к демону `shelldone-agentd`. Он реализует лёгкий протокол на JSON-линиях и
позволяет Shelldone делегировать отдельные запросы внешнему агенту.

## Быстрый старт

```bash
cd agents/openai
python3 -m venv ../../.venv && source ../../.venv/bin/activate
pip install -r requirements.lock
export OPENAI_API_KEY=sk-...
python bridge.py --instructions "Ты аккуратный помощник в терминале"
```

Затем можно отправлять команды в stdin, например:

```bash
echo '{"type": "run", "input": "Сколько сейчас времени?"}' | python bridge.py
```

## Файлы
- `pyproject.toml` — метаданные проекта и entry-point `shelldone-openai-bridge`.
- `requirements.lock` — зафиксированные версии зависимостей.
- `bridge.py` — реализация адаптера (JSON protocol / вызов SDK).
- `__init__.py` — помечает каталог как Python-package.

## Обновление зависимостей
1. Используйте локальный env (`../../.venv/bin/pip`).
2. Обновите зависимости: `../../.venv/bin/pip install --upgrade openai-agents openai`.
3. Зафиксируйте версии: `../../.venv/bin/pip freeze | sort > requirements.lock`.
4. После обновления запустите `make verify` перед коммитом.

## Протокол
- `{"type": "run", "input": "...", "session": "optional"}` — запустить запрос.
- `{"type": "shutdown"}` — корректно завершить процесс.

Ответы:

```json
{"status": "ready", "model": "gpt-4.1-mini"}
{"status": "ok", "output": "..."}
{"status": "error", "error": "описание"}
```

Если `openai-agents` не установлен, адаптер выдаёт структурированную ошибку и
возвращает ненулевой код завершения.

# Agent Governance and Operations

> Обновлено: 2025‑10‑07 · Ответственный: imagray `<magraytlinov@gmail.com>` (support: Automation Guild)

Документ описывает управление внешними агентами (OpenAI, Claude, Microsoft и др.), требования к секретам, механизм smoke-тестов и наблюдаемость. Все процессы выполняются через стандартные скрипты/утилиты репозитория (без внешних CLI-обёрток).

## 1. Жизненный цикл адаптера

1. **Регистрация** — запись в `agents/manifest.json` (id, описание, команда запуска, версия, сигнатура ошибок).
2. **Установка окружения** — разработчик создаёт виртуальную среду `python3 -m venv .venv && source .venv/bin/activate`, запускает `pip install -r requirements.txt` и `npm install` по необходимости. Никаких `.agentcontrol`-капсул.
3. **Smoke** — `python3 scripts/agentd.py smoke` проверяет базовые сценарии (handshake, exec, journal).
4. **Health monitoring** — `shelldone-agentd` пингует адаптеры, события публикуются в telemetry (`agent.adapter.ready`, `agent.adapter.unhealthy`).
5. **Upgrade/Rollback** — обновление делается через PR (обновление manifest/lock). На отказ возвращаем прежнюю версию.

## 2. Secrets
- API ключи (`OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `MICROSOFT_AGENT_API_KEY`) хранятся в системном keychain либо в зашифрованных `.env.enc` (почитайте `docs/security/runbook.md`).
- Скрипт `scripts/agentd.py secrets export` (TODO) подготовит окружение для локального запуска.
- Ротация фиксируется в `state/secrets/ledger.json` (файл создаётся вручную, см. runbook).

## 3. Рабочие команды

| Задача | Команда | Описание |
| --- | --- | --- |
| Smoke тест | `python3 scripts/agentd.py smoke` | Проверяет доступность адаптеров и базовые операции |
| Запустить unit-tests | `cargo test -p shelldone-agentd --lib agent` *(TODO реализовать)* | Контрактные тесты адаптеров |
| Telemetry чек | `python3 scripts/tests/check_otlp_payload.py --payload <file> --snapshot artifacts/termbridge/capability-map.json` | Проверяет OTLP payload на необходимые метрики |

## 4. Наблюдаемость
- **Metrics**: `agent.adapter.ready`, `agent.exec.latency`, `agent.exec.errors`, `agent.policy.denials`.
- **Логи**: `logs/agents.log` (structured JSON).
- **Alerts**: адаптер unhealthy > 2 минут, просроченная ротация секрета (>30 дней), error rate >5 %.
- **Telemetry pipeline**: OTLP → Grafana/Tempo, регулируется скриптом `scripts/tests/check_otlp_payload.py` (используется и для TermBridge).

## 5. Инциденты и playbooks
- Перезапуск `shelldone-agentd`: пока вручную `pkill -f shelldone-agentd` → `shelldone-agentd --foreground` (roadmap: systemd unit).
- Очистка Continuum spool: удалить `state/journal/sigma_guard_spool.jsonl` (действовать осторожно, только после анализа).
- Секреты: см. `docs/security/runbook.md`.

## 6. План развития
1. Добавить контрактные тесты (`tests/agentd_contract.rs`).
2. Подготовить CLI `shelldone agent <cmd>` поверх существующих скриптов.
3. Автоматизировать проверку secrets и health в CI (без внешних CLI-обёрток).
4. Выпустить developer API crate (`shelldone-agent-api`).

Если в процессе обнаружены устаревшие инструкции, обновляйте документ в том же PR вместе с кодом.

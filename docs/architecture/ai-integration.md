# Shelldone Integration with AI Agents and MCP

## Objectives
- Allow terminal control via agent protocols (MCP, OpenAI Assistants, LangChain agents).
- Provide bidirectional context exchange: agents see tabs/processes/files while users govern permissions.
- Deliver a cognitively simple interface for automation and human–AI collaboration.

## Control Protocol
- **Transport:** local gRPC/WebSocket server with optional STDIO/MCP bridge.
- **High-level commands:** `open_tab`, `run_task`, `stream_output`, `apply_theme`, `query_context`.
- **Context payload:** serialised snapshot (panes, processes, paths, pipeline status) with versioned JSON.
- **Security:**
  - Policy files in `config/policies/` define allowed capabilities.
  - Prompt for confirmation on sensitive actions (rm, sudo, deployment, etc.).
  - Optional sandbox (container/namespace) for agent processes.

## Automation
- **Playbooks:**
  - YAML/JSON description of command/API sequences with assertions.
  - Supports parameters, conditionals, retries, notifications.
  - Executable via CLI (`shelldone play run <file>`) or through the agent API.
- **Event hooks:** `on_process_exit`, `on_git_status`, `on_alert`, `on_agent_action`.

## Agent UI
- Dedicated agent status panel (progress, errors, suggested steps).
- Action history with undo/redo.
- “Guided mode”: agent proposes actions, user approves or rejects.

## MCP Compatibility
- MCP tool registry resides in `agents/mcp/tools/`.
- Each integration is packaged as a plugin with declared capabilities.
- Detailed format documentation will live in `docs/agents/mcp.md`.

## Roadmap
1. Define protobuf/JSON schema for the agent API.
2. Prototype the MCP bridge and run a security threat model.
3. Implement the server; expose CLI `shelldone agent serve`.
4. Add UI panel and action log.
5. Ship SDK and sample agents (auto-deploy, coding assistant, observability triage).

## Внешние SDK и адаптеры

Shelldone встраивает два внешних стека агентных SDK. Они поставляются как отдельные адаптеры, которые подключаются к единой шине `shelldone-agentd` (gRPC/MCP). Каждый адаптер реализует контракт `AgentAdapter` (инициализация, трансляция сообщений, управление сессиями) и живёт в собственном каталоге `agents/<vendor>/`.

### OpenAI Agents SDK
- **Источник:** [openai-agents-python](https://github.com/openai/openai-agents-python) — многоагентный каркас с поддержкой Responses/Chat Completions и handoff/guardrail-механизмами.
- **Развёртывание:**
  - Каталог `agents/openai/` содержит `pyproject.toml` и `uv.lock`, которые фиксируют версию `openai-agents` и опциональные экстры (`voice`, `redis`).
  - Runtime стартует как отдельный процесс (venv/uv), общается с Shelldone через локальный gRPC-коннектор `openai_agents_adapter`.
- **Обновление:**
  - Для ручного обновления используем `uv lock --upgrade-package openai-agents` -> `uv sync` -> `make verify`.
  - В CI добавляется периодический job, который выполняет `uv lock --check --frozen` и «падает» при дрейфе.
- **Особенности интеграции:**
  - Используем SDK sessions для сохранения контекста терминала (SQLiteSession по умолчанию, RedisSession по желанию пользователя).
  - Guardrails/hand-offs маппятся на policy-файлы Shelldone и наш механизм approval UI.

### Claude Code Agent SDK
- **Источник:** [@anthropic-ai/claude-code](https://github.com/anthropics/claude-code) — терминальный агент-ассистент с поддержкой natural language git/workflow.
- **Развёртывание:**
  - Каталог `agents/claude/` содержит `package.json` и `package-lock.json`; зависимости (Node.js ≥ 18) управляются через `npm ci`.
  - Запускаем CLI `claude` в headless-режиме и подключаем его STDIO к адаптеру `claude_code_adapter`, который маппит команды на API Shelldone.
- **Обновление:**
  - Ручной апдейт: `npm update @anthropic-ai/claude-code && npm install --package-lock-only` в каталоге адаптера, затем `make verify`.
  - В CI выполняем `npm ci --ignore-scripts` для проверки lock-файла.
- **Особенности интеграции:**
  - Поддерживаем режимы terminal/IDE: адаптер переводит запросы Claude (`/bug`, git-workflow) в унифицированные операции Shelldone.
  - Данные Usage/feedback, которые SDK отправляет в Anthropic, агрегируются и отображаются в панели наблюдаемости (опционально отключается политикой).

### Общие требования к адаптерам
- Жёсткая фиксация версий и воспроизводимость: `uv.lock` и `package-lock.json` — часть репозитория.
- Тестирование: `make verify` вызывает smoke-тесты `agents:<vendor>:test`, которые запускают адаптер в offline-режиме и проверяют hand-off/undo.
- Наблюдаемость: каждый адаптер публикует метрики (`agent.bridge.latency`, `agent.bridge.errors`) в общую систему telemetry (см. `docs/architecture/observability.md`).
- Безопасность: адаптеры читают политики (`config/policies/*.yaml`) и выставляют capabilities при старте; несоответствие политике приводит к отказу запуска.

Related milestones: `docs/ROADMAP/2025Q4.md`, section “AI & Automation”.

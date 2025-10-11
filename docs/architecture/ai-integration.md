# Shelldone Integration with AI Agents and MCP

## Objectives
- Zero-friction automation: агенты получают полный доступ к возможностям Shelldone без повторяющейся настройки.
- Человеческое управление без когнитивного шума: каждая автоматизация прозрачно проходит через политики, Continuum и журналирование.
- Расширяемость: новые агенты и playbooks подключаются декларативно; существующие интеграции получают обновления без ручного вмешательства.

## Experience Pillars
1. **Discoverable by default** — служба обнаружения (`~/.config/shelldone/agentd.json`, mDNS, CLI `shelldone agent discovery`) сообщает порт, TLS и политики.
2. **Context in one hop** — `/context/full` и `mcp.context/full` возвращают typed JSON Schema (`schemas/agent_context.json`), включающую панели, процессы, Continuum, personas, политики.
3. **Automation primitives** — ACK команды (`agent.exec`, `agent.plan`, `agent.batch`, `agent.guard.suggest`) и Playbooks 2.0 заменяют ad-hoc скрипты.
4. **Governance built-in** — mTLS, policy feedback (`rule_id`, remediation), Continuum snapshots и OTLP метрики доступны из коробки.

## Control Protocol
- **Transports**
  - WebSocket MCP (`ws://127.0.0.1:17717/mcp`) — JSON-RPC 2.0 (`initialize`, `tools/list`, `tools/call`, `ping`, heartbeat`).
  - gRPC MCP (`grpc://127.0.0.1:17718`, переопределяется `--grpc-listen`), поддерживает TLS (`--grpc-tls-cert/--grpc-tls-key`) и взаимную аутентификацию (`--grpc-tls-ca`).
  - STDIO адаптеры для SDK (OpenAI, Claude, Microsoft) → `scripts/agentd.py` управляет runtime.
- **TermBridge API** (новый bounded context) — Σ-json/HTTP команды `termbridge.spawn/focus/send_text/duplicate/close/clipboard`. Терминалы описываются через Capability Map (см. `docs/architecture/termbridge.md`); агенты получают:
  - `termbridge.capabilities` — immutable snapshot (`terminal`, `display_name`, `capabilities`, `requires_opt_in`, `risk_flags`, `consent_granted_at`). Снимок формируется один раз per discovery и кэшируется в Continuum; агенты обязаны проверять флаг `requires_opt_in` перед запуском команд.
  - `termbridge.bindings` — список активных binding’ов (`binding_id`, `token`, `labels` с pane/window id).
  - `termbridge.spawn` — создаёт новое окно/панель. CLI пример: `curl -X POST localhost:17717/termbridge/spawn -d '{"terminal":"wezterm","command":"top"}'`. Возвращает `ipc_endpoint` (`wezterm://pane/<id>`), используемый для последующих команд.
  - `termbridge.send_text` — передаёт полезную нагрузку с учётом `bracketed_paste`; при срабатывании PasteGuard возвращает `guard_pending` и Continuum событие `termbridge.paste.guard_triggered`.
  - `termbridge.focus` — переводит фокус на binding; политика проверяет действие `focus`, успех журналируется как `termbridge.focus`, отказ — `termbridge.focus.denied`.
  - `termbridge.clipboard.write/read` (beta) — оборачивают ClipboardBridgeService; поддерживают `channel=clipboard|primary`, backend приоритезацию, журналируют `termbridge.clipboard` метрики; policy ступень `data.shelldone.policy.termbridge_allow`.
  - `termbridge.consent.grant/revoke` (roadmap) — управляет opt-in для удалённого контроля и фиксирует событие `termbridge.consent`.
  - `/status`, `/context/full`, discovery JSON включают `termbridge`-секцию (capabilities, обнаружение, ошибки) для быстрых health-check.
- **Capability Map Semantics**
  - Discoverer собирает факты из адаптеров (kitty `@/listen_on`, wezterm CLI, iTerm2 scripting bridge, Windows Terminal D-Bus/WT.exe, Alacritty IPC, Konsole D-Bus, Tilix session JSON).
  - Карта хранится как Value Object (`CapabilityRecord`), каждое поле валидируется (например, `max_clipboard_kb` ≤ 512).
  - Persona engine получает краткие TL;DR карточки с инструкциями enablement (Nova → пошаговый wizard, Core → ссылочный cheatsheet).
  - PolicyEngine использует `risk_flags[]` (`remote_exec`, `dbus_global`, `no_tls`) для ограничений на команды (запрещает `send_text` без consent, требует подтверждения для `spawn --command`).
- **Operations** — ACK командами управляет `shelldone-agentd` (см. `docs/architecture/utif-sigma.md`). В разработке `agent.batch` для транзакционных последовательностей.
- **Context & Journal**
  - `/context/full` — снимок состояния (Schema, версии, Merkle дельты). Планируется `context.delta` stream.
  - `agent.journal.tail/range` — доступ к Continuum журнальному файлу с spectral tags.
- **Security**
  - Rego policies управляют capability envelopes. Ошибка возвращает `policy_denied` с `rule_id`, `remediation`.
  - gRPC mTLS требует валидный клиентский сертификат; без него сервер отвечает **UNAUTHENTICATED**.
  - Σ-json получит Noise/JWT аутентификацию (roadmap). Пока рекомендуется loopback.
  - Секреты доставляются через `shelldone secrets` и не вытекают в адаптеры.
  - TLS политика (`--grpc-tls-policy`) позволяет выбрать уровень жёсткости шифров: `strict` (только TLS 1.3), `balanced` (TLS 1.3 + FIPS-класс TLS 1.2 при обязательном mTLS) и `legacy` (добавляет CHACHA20 для старых агентов). Смена политики проверяется PolicyEngine и блокируется, если конфигурация нарушает регламент.
  - Сертификаты сервера/клиента читаются из PEM-файлов и поддерживают горячую замену. Любое изменение `--grpc-tls-cert`, `--grpc-tls-key` или `--grpc-tls-ca` подхватывается за ≤5 секунд без остановки процесса; отказ загрузки фиксируется в журнале и не сбрасывает действующие соединения.

#### TLS Policy Matrix

| Policy | Protocol Versions | Cipher Suites (приоритет) | Client Auth | Основные сценарии |
|--------|-------------------|---------------------------|-------------|------------------|
| `strict` | TLS 1.3 | `TLS_AES_256_GCM_SHA384`, `TLS_AES_128_GCM_SHA256`, `TLS_CHACHA20_POLY1305_SHA256` | Optional (CA отключен) | Автономные агенты на том же хосте; минимальный attack surface. |
| `balanced` | TLS 1.3 + TLS 1.2 (ECDHE+AES-GCM) | `strict` + `TLS_ECDHE_[RSA|ECDSA]_WITH_AES_{256,128}_GCM_SHA384` | Требуется, если задан `--grpc-tls-ca` | Стандартный режим: совместимость с корпоративными mTLS-клиентами, при этом Rego требует CA hash match. |
| `legacy` | TLS 1.3 + TLS 1.2 | `balanced` + `TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256` | Требуется при передаче по сети | Для старых клиентов с ограниченной аппаратной поддержкой; включается временно, требует ADR c TTL. |

Все политики зависят от глобально установленного `rustls` provider’а; попытка запуска нескольких `shelldone-agentd` с разными политиками детектируется `python3 scripts/verify.py` (lint `verify_tls_policy_consistency`).

#### Certificate Lifecycle & Runbook

1. **Выпуск**: `shelldone agent tls bootstrap --cn agentd.local --out state/tls/` (roadmap). До появления команды используем `scripts/tls/generate.sh` и складываем PEM в каталог, указанный CLI флагами.  
2. **Горячая ротация**: перезаписываем `cert.pem`/`key.pem`/`ca.pem` атомарно, после чего watcher выполняет debounce (200 мс) и перезапускает gRPC слушатель. SLA на активацию ≤5 с; контроль через метрику `agent.tls.reloads`.  
3. **Валидация**: `curl https://127.0.0.1:17718 -k` должен возвращать `UNAVAILABLE`; `grpcurl --cert client.pem --key client.key --cacert ca.pem localhost:17718 list` — успешный handshake.  
4. **Отказ**: при ошибке парсинга watcher оставляет старую конфигурацию, пишет `agent.tls.reload_errors{reason}`, а CLI `shelldone agent status --tls` (roadmap) отображает последнюю успешную метку времени.  
5. **Rollback**: достаточно восстановить предыдущие PEM из `state/tls/backup/<timestamp>/` и наблюдать метрику `agent.tls.reloads{result="success"}`.

## Automation Surfaces
- **Playbooks 2.0** (roadmap) — YAML с шагами `prepare/run/verify/rollback`, исполняется `shelldone play` и `agent.plan`.
- **Batch ACK** — позволяет агенту отправлять несколько `agent.exec` с зависимостями и единым undo.
- **Persona presets** — `persona.set` (`beginner`, `ops`, `expert`, `nova`, `core`, `flux`) подстраивает подсказки и guard flow.
- **Guard suggestions** — при `policy_denied` Shelldone генерирует `agent.guard.suggest` с описанием remediation.

## Agent UX Enhancements
- `agent.status` поток (Σ-json) передает прогресс, метрики, подсказки.
- Persona Engine публикует `persona.hints` и бюджеты для адаптивных подсказок.
- `agent.inspect` агрегирует `fs`, `git`, `proc`, `telemetry`, экономя команды.
- Контекст и журнал доступны через CLI (`shelldone agent context dump`, `shelldone agent journal tail`).
- TermBridge предоставляет новую группу команд в палитре: «Open here in <Terminal>», «Send text to <Pane>», «Duplicate layout», «Sync clipboard». Команды отображаются только если `CapabilityMap` помечает соответствующие возможности.

## Discovery & Bootstrap
- `shelldone agent discovery` — генерирует registry (host, порт, TLS, политики).
- `shelldone agent profile create --persona ops` — готовит .env, TLS сертификаты, policy значения.
- `shelldone agent tls rotate --cert <path> --key <path> [--ca <path>]` (roadmap) облегчит выпуск новых сертификатов; до появления команды достаточно перезаписать PEM-файлы — фоновый вотчер перезагрузит их автоматически.

#### Context Surfaces

| Surface | Формат | SLA | Реализация | Статус |
|---------|--------|-----|------------|--------|
| `/context/full` | JSON Schema (`schemas/agent_context.json`) | 80 мс p95 | HTTP GET | ✅ |
| `context.delta` (roadmap) | JSON Patch + Merkle proof | 50 мс p95 | WebSocket stream | 🚧 task-context-delta-stream |
| `persona.hints.delta` | JSON Lines | 100 мс p95 | Σ-json push | ✅ |
| `agent.status` | Σ-json metrics snapshot | 1 с | SSE/WebSocket | 🟡 (metrics есть, UI в разработке) |

`context.delta` требует сохранения последовательности ревизий Continuum; реализация ведётся в `task-context-delta-stream`, зависит от Merkle snapshot API и тестов на idempotency.
- Env vars: `SHELLDONE_AGENT_DISCOVERY`, `SHELLDONE_AGENT_PERSONA`, `SHELLDONE_AGENT_POLICY`.
- MCP tool schema доступна через `shelldone agent tools list --format schema`.

## MCP Compatibility Matrix
| Возможность | WebSocket MCP | gRPC MCP | STDIO адаптеры |
|-------------|---------------|----------|----------------|
| `initialize/list/call/heartbeat` | ✅ | ✅ (TLS/mTLS) | ➖ |
| `/context/full` | ✅ | ✅ | ➖ |
| Batch ACK (roadmap) | 🔄 | 🔄 | 🔄 |
| Policy feedback (`rule_id`, remediation) | ✅ | ✅ | ✅ |
| Persona presets | ✅ | ✅ | ✅ |

## Security Checklist
- Обязательный TLS для удалённых агентов; `--grpc-tls-ca` включает строгий mTLS.
- Σ-json auth (Noise/JWT) — в дорожной карте; временно ограничиваемся loopback + локальными токенами.
- Policy denials логируются (`kind: "policy_denied"`), метрики `shelldone.policy.denials`/`evaluations` (Prism).
- Sigma guard события (`sigma.guard`) хранятся в Continuum и доступны агентам через `agent.journal`.
- Полная матрица покрытия болей и roadmap — см. `docs/architecture/pain-matrix.md` (особенно пункты #2, #6, #10, #24, #25).

## Adapter Ecosystem
- **OpenAI Agents SDK** (`agents/openai/`) — Python, STDIO мост.
- **Claude SDK** (`agents/claude/`) — Node.js, STDIO мост.
- **Microsoft Agent SDK** (`agents/microsoft/`) — Node.js, STDIO мост.
- Инварианты и жизненный цикл интеграций описаны в `docs/architecture/agent-sdk-bridge.md` и реализованы доменом `AgentBinding`/`AgentBindingService`.
- Все адаптеры фиксируют версии lock-файлами, проходят smoke-тесты (`python3 scripts/agentd.py smoke`) и экспортируют OTLP метрики (`agent.bridge.latency`, `agent.bridge.errors`).

## Roadmap Highlights (Q4 2025)
1. Streaming `context.delta` + Merkle diff.
2. Playbooks 2.0 + CLI редактор.
3. Persona onboarding wizard для агентов.
4. Σ-json Noise/JWT аутентификация.
5. Developer SDK (`shelldone-agent-api`) и recipes.

## References
- `docs/architecture/utif-sigma.md`
- `docs/architecture/agent-governance.md`
- `docs/architecture/persona-engine.md`
- `docs/recipes/agents.md` (в разработке)

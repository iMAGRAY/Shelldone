# TermBridge Orchestrator and Capability Map

## Purpose
TermBridge предоставляет унифицированный слой управления внешними терминалами/эмуляторами из Shelldone. Он скрывает различия IPC-протоколов (kitty, WezTerm, iTerm2, Windows Terminal, Alacritty, Konsole, Tilix и др.), обеспечивает автодетект возможностей, безопасную оркестрацию операций (spawn/split/focus/send-text/clipboard) и когерентный пользовательский опыт.

## Goals
- **Zero-friction orchestration.** Пользователь и агент работают с абстрактной моделью окон/панелей независимо от выбранного терминала.
- **Safety-first remote control.** Все операции идут через карту возможностей и политику безопасности; потенциальные опасные действия требуют явного согласия.
- **Predictable UX.** Синхронизация CWD, bracketed paste, clipboard bridge и контекстные команды всегда ведут себя одинаково.
- **Observability & Audit.** Каждый orchestration action протоколируется (Continuum + OTLP метрики), позволяя расследовать инциденты.

## Architecture Alternatives & Decision

| Alternative | Correctness (0.35) | Performance (0.25) | Simplicity (0.20) | Evolvability (0.15) | Cost (0.05) | Weighted Score |
|-------------|--------------------|---------------------|-------------------|---------------------|-------------|----------------|
| **A. Тонкие обёртки над CLI каждого терминала, без оркестратора** | 0.60 | 0.55 | 0.70 | 0.40 | 0.80 | **0.60** |
| **B. TermBridge orchestrator + Capability Map (выбранное)** | 0.92 | 0.86 | 0.78 | 0.90 | 0.65 | **0.86** |
| **C. Плагин-посредник в каждом терминале (инъекция JS/Lua/доп. демонов)** | 0.75 | 0.68 | 0.40 | 0.72 | 0.30 | **0.63** |

**Pareto front:** альтернативы A и B. A имеет лучшие показатели по стоимости, но уступает по корректности и эволюционированию; C доминируется обеими и исключается. Выбрана альтернатива B, потому что Capability Map позволяет кодогенерации UX, централизованным политикам и «безумно лёгкой» когнитивной модели для агентов. Альтернатива A отвергнута: отсутствие единого состояния ломает policy enforcement и приводит к дублированию логики в каждом адаптере. Альтернатива C отвергнута: требует внедрения кода в терминалы, увеличивает attack surface и сложность сопровождения.

## Capability Map Schema

| Field | Type | Description |
|-------|------|-------------|
| `terminal` | `TerminalId` | Каноническое имя адаптера (`kitty`, `wezterm`, `konsole`, …). |
| `display_name` | string | Человекочитаемый ярлык для UI/палитры. |
| `version` | string (optional) | Версия клиента, возвращаемая discovery. |
| `capabilities.spawn` | bool | Поддерживается ли создание новых окон/панелей. |
| `capabilities.split` | bool | Доступны ли сплиты внутри окна. |
| `capabilities.focus` | bool | Возможность перевести фокус на указанное окно/панель. |
| `capabilities.duplicate` | bool | Поддерживается ли дублирование существующей панели (split/new tab). |
| `capabilities.close` | bool | Может ли адаптер закрыть ранее созданную связь. |
| `capabilities.send_text` | bool | Разрешена ли отправка текста (учитывая consent). |
| `capabilities.clipboard_write/read` | bool | Настоящие операции OSC 52 или OS clipboard. |
| `capabilities.cwd_sync` | bool | Терминал принимает OSC 7/9;9. |
| `capabilities.bracketed_paste` | bool | Точно ли терминал корректно обрабатывает bracketed paste. |
| `capabilities.max_clipboard_kb` | integer? | Верхний порог полезной нагрузки до fallback/батчинга. |
| `requires_opt_in` | bool | Требуется ли явное включение пользователя (kitty listen-on, iTerm2 API). |
| `consent_granted_at` | RFC3339? | Момент, когда пользователь разрешил управление (audit trail). |
| `source` | string | Происхождение записи (`local`, `mcp`, `bootstrap`, `external`). |
| `notes[]` | array<string> | Причины деградации, подсказки оператору. |
| `risk_flags[]` | array<string> | Маркеры безопасности (`"remote_exec"`, `"no_tls"`, `"dbus_global"`). |

Capability Map хранится в `TermBridgeState` (domain aggregate), публикуется в Σ-json (`/termbridge/capabilities`) и Continuum (`termbridge.capability.update{source,change}`). Любые преобразования происходят в Application Service, соблюдая неизменяемость Value Object.

## Bounded Contexts (DDD)
```
+--------------------+        +-----------------------+
| TermBridge Domain  |        | Terminal Adapters     |
|--------------------|        |-----------------------|
| CapabilityMap      |<------>| kitty_adapter         |
| TerminalBinding    |        | wezterm_adapter       |
| SessionProfile     |        | iterm2_adapter        |
| ClipboardPolicy    |        | windows_terminal      |
| PasteGuardPolicy   |        | alacritty_adapter     |
+--------------------+        | konsole_adapter       |
        ^                    | tilix_adapter          |
        |                    +-----------------------+
        | uses via port
+--------------------+
| Application Layer  |
|--------------------|
| TermBridgeService  |
| ClipboardBridgeSvc |
| PasteGuardSvc      |
| CwdSyncSvc         |
+--------------------+
```

### Domain Objects
- **CapabilityMap** — immutable snapshot возможностей терминала (`supports_split`, `supports_focus`, `supports_clipboard_write`, `max_payload`, `requires_opt_in`, и т.д.).
- **TerminalBinding** — связь Shelldone session ↔ внешнего окна/панели (`binding_id`, `token`, `ipc_endpoint`, `pid`, `platform`).
- **SessionProfile** — активный контекст (persona, CWD, env). Используется для синхронизации OSC 7/9;9.
- **ClipboardPolicy / PasteGuardPolicy** — политики безопасности (порог длины, эвристики suspicious paste, bracketed state).

### Application Services
- **TermBridgeService** — публичный порт (Σ-json + CLI) для команд `termbridge.spawn`, `termbridge.focus`, `termbridge.send_text`, `termbridge.duplicate`, `termbridge.close`; держит кэш последнего discovery и fallback на `discover()` при пустом состоянии.
- **ClipboardBridgeService** — orchestrates OSC 52 / wl-copy / xclip / Windows API, policy-gated через `data.shelldone.policy.termbridge_allow`.
- **PasteGuardService** — анализирует вставки, применяет bracketed-paste, запускает UX overlay.
- **CwdSyncService** — публикует OSC 7 и 9;9, обновляет Continuum (`cwd.update`).

### Domain Events
- `termbridge.capability.update` — Capability Map изменился; триггерит UI refresh и persona hints (`change=added|updated|removed`).
- `termbridge.binding.created`/`termbridge.binding.lost` — Binding lifecycle, ссылается на `TerminalBindingId`.
- `termbridge.action.accepted/denied` — каждое действие (spawn/send_text/focus) фиксирует latency, persona, policy decision.
- `termbridge.duplicate`/`termbridge.duplicate.denied` — создание сплит-панели из существующей связи, содержит `strategy`, `new_binding_id`, `ipc_endpoint`.
- `termbridge.close`/`termbridge.close.denied` — завершение связи, фиксирует исходный `binding_id`, `terminal`, `token` и результат политики.
- `termbridge.clipboard.write/read/denied` — фиксируют Transfer/denial (payload bytes, backend, persona, policy).
- `termbridge.paste.guard_triggered` — PasteGuard запросил подтверждение; payload содержит эвристику (`"newline"`, `"zero_width"`).

### Ports & Adapters
Каждый терминал реализуется как Hexagonal adapter, соответствующий интерфейсу `TerminalControlPort`:
- `spawn(args: SpawnRequest) -> TerminalBinding`
- `send_text(binding, payload, PasteMode)`
- `focus(binding)`
- `duplicate(binding, options)`
- `supported_capabilities() -> CapabilityMap`

#### REST / Σ-json Surface
- `POST /termbridge/spawn` — принимает JSON `{ "terminal": "wezterm", "command": "htop", "cwd": "/tmp", "env": {"FOO": "BAR"} }`. Возвращает Binding summary (`id`, `terminal`, `token`, `labels`, `ipc_endpoint`, `created_at`).
- `POST /termbridge/duplicate` — `{ "binding_id": "…", "strategy": "horizontal_split" | "vertical_split" | "new_tab" | "new_window", "command": "htop", "cwd": "/tmp" }`. Возвращает summary новой связи; стратегия по умолчанию — `horizontal_split`.
- `POST /termbridge/send-text` — `{ "binding_id": "…", "payload": "ls -la\n", "bracketed_paste": true }`.
- `POST /termbridge/close` — `{ "binding_id": "…" }`. Закрывает связь, удаляет её из репозитория и публикует событие `termbridge.close`.
- `GET /termbridge/bindings` — список активных связей, включая `ipc_endpoint` (например `wezterm://pane/42`).
- Все маршруты требуют успешного `termbridge.capabilities` discovery: TermBridgeService поднимает snapshot из кэша либо выполняет `discover()` при первом вызове.
- `POST /termbridge/discover` возвращает payload `{"last_discovery_at": ..., "terminals": [...], "clipboard_backends": [...], "changed": bool, "diff": {"added": [...], "updated": [...], "removed": [...]}}`. Списки diff содержат тот же DTO, что и `terminals`, и заполняются только при изменениях capability map.

#### Kitty Adapter
- IPC: `kitty @ --to unix:/path socket ...`.
- Требует `listen-on` от пользователя. При отсутствии — capability `remote_control=false`.
- Security: whitelist команд (`set-font`, `new-window`, `close-window`, etc.).

#### WezTerm Adapter
- **CLI path resolution.** По умолчанию используется `wezterm` из `$PATH`; для детерминированных окружений поддержан override `SHELLDONE_TERMBRIDGE_WEZTERM_CLI=/abs/path/to/cli`. Discovery отражает override (notes: `using override`, `override missing`).
- **Spawn.** `wezterm cli spawn --format json` возвращает pane/window/tab id. TermBridge сохраняет `pane_id` и `ipc_endpoint=wezterm://pane/<id>`. Для legacy CLI без JSON fallback — токен из stdout + note `legacy_spawn_output` в capability map.
- **Focus.** `wezterm cli activate-pane --pane-id <id>` выполняется через единый command runner (таймаут 3 с, stderr capture). Успех → событие `termbridge.focus`, denial → `termbridge.focus.denied`/`termbridge.errors{reason}`.
- **Send text.** `wezterm cli send-text --pane-id <id> --text <payload>` с bracketed paste по умолчанию (`--no-paste` при `bracketed_paste=false`). Payload всегда фиксируется в Continuum (scrubbed) для аудита.
- **Command runner.** Все wezterm операции проходят через общий раннер: структурированные ошибки, повторное использование timeout-константы, преобразование `MissingCli` → HTTP 501 (`TermBridgeServiceError::NotSupported`).

#### iTerm2 Adapter
- IPC: Python/WebSocket API. Включается пользователем (off by default).
- TermBridge запрашивает consent → persona overlay; binding хранит auth token.

#### Windows Terminal Adapter
- IPC: `wt.exe` (`duplicateTab`, `splitPane`, `focusPane`).
- CWD: `wt.exe -d`, fallback `%CD%`, OSC 9;9.

#### Alacritty Adapter
- IPC: `alacritty msg create-window --working-directory`. Ограниченные возможности (нет split/focus). Capability map помечает unsupported.

#### Konsole Adapter
- D-Bus методы (`org.kde.konsole.Window`/`Session`).
- Требует DBus session bus; binding хранит window/session id.

#### Tilix Adapter
- CLI + layout JSON (`tilix --session`). Ограничения (нет прямого focus) отражены в capability map.

### Capability Detection Flow
1. TermBridge запускает discovery: сканирует известных терминалов, проверяет наличие сокетов/CLI.
2. Для каждого кандидата adapter возвращает `CapabilityObservation` (подтверждённые/требующие opt-in/unsupported).
3. Формируется `CapabilityMap` → сохраняется в `TermBridgeStateRepository` (persisted snapshot + in-memory cache, файл `state/termbridge/capabilities.json`) и публикуется через `/status`, `/context/full`, discovery JSON.
4. Persona engine использует map для подсказок (Beginner → показать TL;DR карточку).

### Discovery Outcome Contract
- Application Service предоставляет `TermBridgeService::discover(source) → TermBridgeDiscoveryOutcome`.
- `state` — актуальный `TermBridgeState`, синхронизированный с репозиторием и кешем.
- `diff` — `TermBridgeDiscoveryDiff` с отсортированными списками `added`, `updated`, `removed` (по `terminal`).
- `changed` — булево, фиксирует факт обновления Capability Map `state_repo` и инвалидации кеша.
- Телеметрия: непустой `diff` создаёт события `termbridge.capability.update{terminal,source,change}` и инкрементирует `termbridge.actions{command="discover",outcome="changed"}`; `diff=∅` → outcome `noop`.
- HTTP ответ `/termbridge/discover` отражает `changed` и `diff` синхронно с Outcome; `GET /termbridge/capabilities` возвращает `changed=false` и пустой diff.
- Идемпотентность: повторный `discover` без изменений возвращает `changed=false`, `diff=∅`, что позволяет API использовать snapshot без повторной записи.

### Runtime Configuration
- `SHELLDONE_TERMBRIDGE_MAX_INFLIGHT` — количество одновременных операций (spawn/send_text/focus) на TermBridge; по умолчанию 32, значения ≤0 приводят к использованию дефолта.
- `SHELLDONE_TERMBRIDGE_QUEUE_TIMEOUT_MS` — тайм-аут ожидания семафора перед ошибкой `Overloaded`; по умолчанию 5000 мс.
- `SHELLDONE_TERMBRIDGE_DISCOVER_CACHE_MS` — TTL кеша ответа `/termbridge/discover`; по умолчанию 1000 мс (0 → кеш отключён). GUI и агенты многократно вызывают discover, поэтому значение не должно превышать период фонового discovery (30 с).

### Discovery Drift Control
- Фоновый `TermBridgeDiscovery` пересканирует адаптеры каждые 30 с и по MCP событиям (`mcp.session.established`, `mcp.session.closed`).
- Новые терминалы попадают в capability map < 30 с, устаревшие записи удаляются ≤ 60 с.
- Все обновления логируются как `termbridge.capability.update` (`change=added|updated|removed`) и доступны Experience Hub / Sigma.
- `discover()` опрашивает адаптеры параллельно (Tokio `FuturesUnordered`), среднее время полного сканирования на Linux — **1.3 мс** (замер от 2025‑10‑10 через `scripts/tests/termbridge_matrix.py`, артефакт `dashboards/artefacts/termbridge/linux.json`).

### Discovery Registry Service
- **Domain aggregate.** `TerminalRegistry` управляет версией Capability Map и списком терминалов. Он фиксирует события `TerminalDiscovered`, `TerminalRemoved`, `TerminalBlocked` и синхронизируется с Continuum для идемпотентности.
- **Boot sequence.** При старте agentd registry читает bootstrap (`config/termbridge/bootstrap.yaml`) и публикует `termbridge.capability.update{source="bootstrap",change="added"}`. Далее MCP watcher (`task-termbridge-discovery-mcp-sync`) применяет дельты без рестарта, вызывая `TerminalRegistry::apply_delta`.
- **Policy guard.** Перед включением терминала registry вызывается Rego `data.shelldone.termbridge.allow_discovery`. Заблокированные терминалы отмечаются `status=blocked`, UI показывает причину (audit trail для security).
- **Persistence.** Registry сбрасывает состояние в `$STATE/termbridge/registry.json` (версия, consent, capability overrides). Это позволяет восстановиться после крэша и показать текущее состояние в `see docs/status.md`.
- **Telemetry.** Каждое обновление эмитит span `termbridge.discovery.sync` и метрику `termbridge.capabilities.discovered{terminal,source}`; dashboards отслеживают прирост терминалов и дельты. Эти метрики закрывают задачи `task-termbridge-discovery-registry` и `task-termbridge-core-telemetry`. Smoketest `scripts/tests/termbridge_otlp_smoke.py` (таргет `make termbridge-telemetry-smoke`) гарантирует, что `check_otlp_payload.py` подтверждает наличие `terminal/source/change`; артефакты складываются в `reports/roadmap/termbridge/termbridge-telemetry/`.

### MCP Sync Workflow
- **Port & adapter.** `TermBridgeSyncPort` объявляет контракт `apply_external_snapshot(snapshot, source)`; реализация `McpBridgeService` получает обновлённую capability map через инструмент `termbridge.sync` и транслирует её в домен. Проверка выполняется по capability claim: если сессия MCP не объявила `termbridge.sync`, `call_tool` завершается `PermissionDenied`.
- **Capability map sources.** Снимки приходят из двух каналов: (a) `TermBridgeDiscovery` — локальные опросы адаптеров; (b) `termbridge.sync` — удалённые агенты/IDE. `TerminalRegistry` маркирует каждую запись `source=local|mcp|bootstrap|external`, UI/персона получают поле `source` и могут различать происхождение возможностей. При конфликте приоритет у `local`, но удалённый снапшот сохраняется как pending и публикуется после подтверждения discover (зашито в `TerminalSnapshot::is_superseded_by(discover_hwm)`).
- **Watcher pipeline.** MCP backend выполняет gRPC вызов `termbridge.sync` → `McpBridgeService::handle_sync()` → `TermBridgeSyncPort::apply_external_snapshot()`. Последний вызывает `TermBridgeService::apply_external_snapshot` внутри Tokio task, публикуя Domain Event `TerminalSyncApplied`. Через transactional outbox событие попадает в OTLP (`termbridge.sync.applied`) и Experience Hub получает уведомление.
- **Crash & idempotency.** Каждому снапшоту присваивается `sync_id` (uuid) и `capability_version`. Registry хранит `last_applied_sync` в `$STATE/termbridge/registry.json`; повторы с тем же `sync_id` игнорируются. При крэше незавершённые снапшоты откатываются: до фиксации `TerminalSyncApplied` дельта находится в staging и не влияет на публичную карту.
- **Telemetry & budgets.** Каждый sync записывает peak RSS/latency в `reports/logs/*-termbridge-sync.log` (см. QA Harness). Бюджет: p95 < 150 мс, peak RSS < 64 МиБ. Превышение вызывает `termbridge.sync.budget_exceeded` и блокирует приём новых снапшотов до ручного подтверждения.
- **Integration coverage.** gRPC end-to-end сценарий `tests/e2e_mcp_grpc.rs::termbridge_sync_applies_remote_snapshot` подтверждает, что инструмент публикует snapshot, возвращает корректный diff (`added/removed`) и сохраняет state в `state/termbridge/capabilities.json`.

### Feasibility & UX Impact Matrix

| Capability | Реализация в Shelldone | UX эффект | Комментарии |
|------------|------------------------|-----------|-------------|
| Единый оркестратор `termbridged` + карта возможностей | **Да.** TermBridgeService, Capability Map, adapters для kitty/wezterm/WT/iTerm2/Alacritty/Konsole/Tilix. | Агенты видят универсальный API, UI отображает только допустимые действия. | Требует расширить adapters для полного IPC (см. roadmap `task-termbridge-core`). |
| Синхронизация CWD (OSC 7, OSC 9;9) | **✅ Completed.** `CurrentWorkingDirectory` value object + `/termbridge/cwd` REST endpoint with policy guard; shell hooks (`bash/zsh/fish/pwsh/cmd`) live in `scripts/shell-hooks/` (tracked under `task-termbridge-shell-hooks`). | «Открыть здесь» всегда попадает в нужную директорию, агенты получают актуальный контекст. | Требуется кросс-платформенные скрипты и tmux passthrough. |
| Bracketed paste pipeline + PasteGuard | **В процессе.** PasteGuardService, эвристики и overlay описаны; UX прототип в roadmap `task-termbridge-paste-guard`. | Исключает разрушение Vim/REPL, снижает риск неявных команд. | Поддержка в adapters: kitty/wezterm (auto), WT (CLI `sendInput`). |
| Clipboard bridge (OSC 52, wl-copy, xclip, Windows API) | **Готово (beta).** ClipboardBridgeService с системными backend’ами, метрики `termbridge.clipboard.bytes`. | Единая команда копирования, работает в Wayland/X11/Windows; tmux passthrough → backlog. | Guard на OSC 52 лимиты и policy explain в работе. |
| Безопасность управляющих последовательностей | **Да.** ANSI sanitizer в σ-proxy + policy guard; Continuum не хранит сырые ESC. | Исключает XSS/alert fatigue, журнал чистый. | Требует fuzz-тестов (`task-termbridge-security`). |
| Гарды удалённого управления (consent toggles, scopes) | **Да.** Capability Map `requires_opt_in`, Rego `termbridge.allow`. UI overlay хранит consent. | Пользователь явно включает управление, видно в логах. | Для kitty/iTerm2 показываем TL;DR карточки с инструкциями. |
| Идентификация окон/табов без гонок | **Да.** `TerminalBinding` хранит token + labels (pane_id, window_id). | Команды не «стреляют» в другое окно. | Требует heartbeat/validation в adapters. |
| Надёжность и таймауты | **Да.** Каждый IPC вызов с таймаутом (default 3 с), retry=2, fallback сообщение. | UX получает понятные ошибки, не «подвисает». | Таймауты конфигурируемы (`SHELLDONE_TERMBRIDGE_TIMEOUT_MS`). |
| Тест-набор совместимости | **Готово (Windows/macOS CI).** `scripts/tests/termbridge_matrix.py` выполняется на `termbridge-matrix` workflow с реальным WezTerm CLI. | Регрессии ловятся автоматически, snapshot доступен в artifacts. | Для Linux остаётся контейнеризованный прогон (`task-termbridge-test-suite`).

### Security & Policy
- Каждая операция проходит через Rego policy (`data.shelldone.termbridge.allow`). Вход: команда, persona, terminal, capability flags, риск-метки.
- Опасные команды (spawn with remote command, broadcast paste) требуют `approval_granted`. Persona Nova получает guided overlay.
- Remote control API (iTerm2/kitty) по умолчанию disabled. Для enable → пользователь включает toggle, TermBridge фиксирует `consent` флаг в CapabilityMap, Logging: `termbridge.consent`.
- Action logging: `EventRecord::new("termbridge.action", …)` хранит `binding_id`, `command`, `args`, latency. Для ошибок — `termbridge.error`.
- ANSI sanitization: любые входящие строки проходят фильтр прежде чем попасть в Continuum/логи.
- Discovery эндпоинт `/termbridge/discover` защищается опциональным Bearer-токеном: при наличии `SHELLDONE_TERMBRIDGE_DISCOVERY_TOKEN` daemon требует заголовок `Authorization: Bearer …`, GUI считывает токен из `SHELLDONE_AGENTD_DISCOVERY_TOKEN`, а HTTP-транспорт разрешается только при `SHELLDONE_GUI_ALLOW_INSECURE_AGENTD=1` (diagnostic режим).

### Current Working Directory Sync
- **Domain contract.** `CurrentWorkingDirectory` (immutable value object) enforces length ≤4096, no control characters, valid path components. Aggregates expose `TerminalBinding::set_cwd` and `TerminalBinding::cwd()` to keep label updates explicit.
- **Application service.** `TermBridgeService::update_cwd` loads the binding, applies the value object, persists via `TerminalBindingRepository`, and emits Prism metrics (`termbridge.update_cwd`, `termbridge.error`). Missing bindings raise `TermBridgeServiceError::NotFound` → HTTP 404.
- **Interface.** `POST /termbridge/cwd` accepts `{binding_id, cwd}`; input is policy-gated с действием `cwd.update`, persona контекстом, terminal id и журналируется как `termbridge.cwd_update`. Denials → `termbridge.cwd_update.denied` с пояснениями Rego. `POST /termbridge/focus` принимает `{binding_id}` и инициирует перевод фокуса; policy проверяет действие `focus`, успех приводит к событию `termbridge.focus`, отказ фиксируется как `termbridge.focus.denied`.
- **Policy.** Default Rego adds `cwd.update` to the allowlist and limits path length; failures surface through `termbridge_deny_reason` including offending `cwd` for telemetry triage.
- **Verification.** Regression coverage lives in `termbridge` unit tests (`CurrentWorkingDirectory`, `TerminalBinding::set_cwd`, service update) and policy tests (`policy_termbridge_allows_cwd_update`, `policy_termbridge_denies_oversized_cwd_update`). Run `cargo test -p shelldone-agentd termbridge` and `opa eval` via `python3 scripts/verify.py` gates.
- **Capability Snapshot Export.** `shelldone-agentd --termbridge-export <path>` выполняет discovery без запуска демона, возвращает Capability Map (`terminals`, `diff`, `totals`, `clipboard_backends`) и используется CI-скриптом `scripts/tests/termbridge_matrix.py` для валидации документации/roadmap. Артефакт по умолчанию — `artifacts/termbridge/capability-map.json`.
- **CI покрытие.** Workflow `.github/workflows/termbridge_matrix.yml` гоняет snapshot на `macos-latest`, `windows-latest` и `ubuntu-latest`, перед запуском устанавливает WezTerm (brew/choco/apt+deb), выполняет `wezterm --version` smoke-check и подтверждает, что `termbridge.capability.update` содержит для каждого терминала datapoint с `change ∈ {added, updated}`, ненулевым значением и непустым `source`. Результат зеркалируется в `dashboards/artefacts/termbridge/<os>.json`, а baseline `dashboards/baselines/termbridge/monitored_capabilities.json` обеспечивает auto-alert: при дрейфе ≥1 способности workflow падает и пишет diff в `<os>-drift.json`.

### UX Patterns
- **Context palette:** операции `Open here in <Terminal>`, `Send text to <Pane>`, `Duplicate layout`, `Attach clipboard`. Пункты отображаются только если capability `supported=true`.
- **CWD sync:** shell hooks (bash/zsh/fish/pwsh/cmd) из `scripts/shell-hooks/` отправляют OSC 7/9;9; CwdSyncSvc обновляет Continuum и UI (breadcrumbs). Фокусировка панелей (`/termbridge/focus`) синхронизируется с UI, чтобы палитра не предлагала неактивные binding’и.
- **Paste guard:** при мультилайн/невидимых символах → overlay (Nova/Core), bracketed paste всегда соблюдается, whitespace нормализуется, optional confirm.
- **Clipboard:** default OSC 52, fallback Wayland/X11/SSH/tmux; при превышении 75 KB — batching, user feedback.
- **Experience overlay:** раздел Layout Summary (Experience Hub) показывает доступные действия и поле `source`, чтобы отличать локальные и внешние capability snapshots (например, `mcp`).

### ClipboardBridgeService (beta)
- Доступные backend’ы по умолчанию: `wl-copy/wl-paste` (Wayland), `xclip` (X11), `clip.exe` + `powershell.exe Get-Clipboard` (Windows/WSL), `pbcopy/pbpaste` (macOS). Построены на командном адаптере с таймаутами и telemetry (`termbridge.clipboard.bytes`).
- REST API: `POST /termbridge/clipboard/write` (`text` или `base64`, `channel`, `backend`) и `POST /termbridge/clipboard/read` (`channel`, `backend`, `as_base64`). Агентам доступны те же операции через MCP (`termbridge.clipboard.write/read`, roadmap).
- Политики: Rego `data.shelldone.policy.termbridge_allow` ограничивает каналы/размер; persona Nova/Flux требует явное подтверждение, deny события пишутся как `termbridge.clipboard.denied`.

### Observability
- Метрики: `termbridge.actions`, `termbridge.errors`, `termbridge.latency_ms`, `termbridge.clipboard.bytes`, `termbridge.paste.guard_tripped`, `termbridge.capabilities.discovered`. Экспортируются через OTLP.
- Continuum события: `termbridge.action`, `termbridge.error`, `termbridge.capability.update`, `cwd.update`, `clipboard.transfer`.
- Alerts: error rate > 2% → Ops, paste guard trip rate > 5/min → UX review.

### Failure Handling
- Все IPC вызовы с таймаутом (default 3s) и retry (2 попытки). При ошибке → degrade (сообщение + fallback: shell command `tmux split-window`, `ssh ...`).
- Отвязанные binding (terminated window) → событие `termbridge.binding.lost`, UI показывает диагностику.
- Безопасность: если capability требует opt-in, TermBridge не выполняет действие до явного consent.
- Ошибки `NotSupported` (например, отсутствующий CLI) конвертируются в HTTP 501 и метку `termbridge.errors{reason="not_supported"}`, что предотвращает silent fallback и ускоряет triage.
- **Backpressure.** `TermBridgeService` пропускает одновременно не более `max_inflight` операций (default 32). Попытки сверх лимита ждут `queue_timeout_ms` (default 5000) и, если очередь не освободилась, получают `TermBridgeServiceError::Overloaded` → HTTP 429 и метрику `termbridge.errors{reason="overloaded"}`. Параметры управляются через env-переменные `SHELLDONE_TERMBRIDGE_MAX_INFLIGHT` и `SHELLDONE_TERMBRIDGE_QUEUE_TIMEOUT_MS`. Гравитация правил: backpressure распространяется на `spawn`, `send_text`, `focus`; permit удерживается только на время IPC-вызова, чтобы не блокировать state-операции.

- **Discovery cache TTL.** Снимок Capability Map кэшируется в `TermBridgeService` с TTL `SHELLDONE_TERMBRIDGE_SNAPSHOT_TTL_MS` (default 60000). По истечении TTL `snapshot()` принудительно запускает «живую» discovery (source=`snapshot_ttl_expired`), обновляя state и метрики. Дополнительно HTTP‑эндпоинт `/termbridge/discover` имеет независимый TTL `SHELLDONE_TERMBRIDGE_DISCOVER_CACHE_MS` для ответа.

## Consent API (Opt‑in Terminals)

Некоторые адаптеры требуют явного согласия пользователя (например, iTerm2 Python API, Kitty remote-control). В таких случаях TermBridge блокирует действия до получения consent и ведёт аудит.

- Контракты:
  - `POST /termbridge/consent/grant` — тело: `{ "terminal": "<id>" }` → `{ "success": true }`.
  - `POST /termbridge/consent/revoke` — тело: `{ "terminal": "<id>" }` → `{ "success": true }`.
  - Состояние хранится атомарно в `state/termbridge/consent.json` (tmp→rename), формат: `{ "granted": ["wezterm", "kitty"] }`.

- Инварианты безопасности:
  - Для action из множества `{spawn, duplicate, send_text, focus, close, cwd.update}` при `requires_opt_in=true` и отсутствии consent → HTTP 403 `{ code: "consent_required" }`.
  - События: `termbridge.<action>.denied` с причиной `consent_required`; метрика `shelldone.policy.denials{command="termbridge.<action>"}`.
  - Политики Rego получают поля `requires_opt_in` и `consent_granted` (input), что позволяет централизованно переопределять правила.

- UX и диагностика:
  - `/termbridge/capabilities` отражает `requires_opt_in` по каждому терминалу; UI предлагает «Grant consent» при первом использовании.
  - Ревок сохраняет безопасность: любые активные действия прекращаются, новые блокируются до повторного grant.


### Testing Strategy
- **Unit:** CapabilityMap merge/reconcile, PasteGuard heuristics, Clipboard batching.
- **Integration (per terminal):** spawn/tab/split/focus/send_text/clipboard/CWD/paste guard/OSC filter. Использовать headless режимы (kitty/wezterm) и CI runner матрицу (Linux, macOS, Windows).
- **Contract:** verify bracketed paste, OSC 7/9;9, clipboard fallback, tmux passthrough.
- **Security:** fuzz escape sequences, ensure sanitization, Rego policy denies unsafe commands.
- **Command runner:** unit-тесты для wezterm адаптера используют in-memory runner, проверяющий аргументы (`activate-pane`, `--no-paste`) и обработку `MissingCli` → NotSupported.

### Roadmap / Tasks
- `task-termbridge-core` — минимальный orchestrator + capability map + kitty/wezterm adapters.
- `task-termbridge-clipboard` — clipboard pipeline (OSC 52 + wl-copy/xclip).
- `task-termbridge-paste-guard` — heuristics, overlays, persona integration.
- `task-termbridge-discovery` — автодетект терминалов и TL;DR карточки.
- `task-termbridge-test-suite` — e2e matrix, CI runners.
- `task-termbridge-security` — Rego policy, consent toggles, audit logging.
- `task-termbridge-windows` — WT adapter, OSC 9;9, quoting.
- `task-termbridge-dbus` — Konsole/Tilix adapters, layout sync.

### References
- kitty remote control — <https://sw.kovidgoyal.net/kitty/remote-control/>  
- WezTerm CLI — <https://wezterm.org/cli/send-text.html>  
- iTerm2 API — <https://iterm2.com/documentation-python-api.html>  
- Windows Terminal OSC 9;9 — <https://learn.microsoft.com/windows/terminal/command-line-arguments#startingdirectory>  
- OSC 52 spec — <https://sunaku.github.io/tmux-yank-osc52.html>

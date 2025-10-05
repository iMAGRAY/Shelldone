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
| `capabilities.send_text` | bool | Разрешена ли отправка текста (учитывая consent). |
| `capabilities.clipboard_write/read` | bool | Настоящие операции OSC 52 или OS clipboard. |
| `capabilities.cwd_sync` | bool | Терминал принимает OSC 7/9;9. |
| `capabilities.bracketed_paste` | bool | Точно ли терминал корректно обрабатывает bracketed paste. |
| `capabilities.max_clipboard_kb` | integer? | Верхний порог полезной нагрузки до fallback/батчинга. |
| `requires_opt_in` | bool | Требуется ли явное включение пользователя (kitty listen-on, iTerm2 API). |
| `consent_granted_at` | RFC3339? | Момент, когда пользователь разрешил управление (audit trail). |
| `notes[]` | array<string> | Причины деградации, подсказки оператору. |
| `risk_flags[]` | array<string> | Маркеры безопасности (`"remote_exec"`, `"no_tls"`, `"dbus_global"`). |

Capability Map хранится в `TermBridgeState` (domain aggregate), публикуется в Σ-json (`/termbridge/capabilities`) и Continuum (`termbridge.capability.update`). Любые преобразования происходят в Application Service, соблюдая неизменяемость Value Object.

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
- `termbridge.capability.update` — Capability Map изменился; триггерит UI refresh и persona hints.
- `termbridge.binding.created`/`termbridge.binding.lost` — Binding lifecycle, ссылается на `TerminalBindingId`.
- `termbridge.action.accepted/denied` — каждое действие (spawn/send_text/focus) фиксирует latency, persona, policy decision.
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
- `POST /termbridge/send-text` — `{ "binding_id": "…", "payload": "ls -la\n", "bracketed_paste": true }`.
- `GET /termbridge/bindings` — список активных связей, включая `ipc_endpoint` (например `wezterm://pane/42`).
- Все маршруты требуют успешного `termbridge.capabilities` discovery: TermBridgeService поднимает snapshot из кэша либо выполняет `discover()` при первом вызове.

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
3. Формируется `CapabilityMap` → сохраняется в `TermBridgeStateRepository` (persisted snapshot + in-memory cache) и публикуется через `/status`, `/context/full`, discovery JSON.
4. Persona engine использует map для подсказок (Beginner → показать TL;DR карточку).

### Discovery Registry Service
- **Domain aggregate.** `TerminalRegistry` управляет версией Capability Map и списком терминалов. Он фиксирует события `TerminalDiscovered`, `TerminalRemoved`, `TerminalBlocked` и синхронизируется с Continuum для идемпотентности.
- **Boot sequence.** При старте agentd registry читает bootstrap (`config/termbridge/bootstrap.yaml`) и публикует `termbridge.capability.update{source="bootstrap"}`. Далее MCP watcher (`task-termbridge-discovery-mcp-sync`) применяет дельты без рестарта, вызывая `TerminalRegistry::apply_delta`.
- **Policy guard.** Перед включением терминала registry вызывается Rego `data.shelldone.termbridge.allow_discovery`. Заблокированные терминалы отмечаются `status=blocked`, UI показывает причину (audit trail для security).
- **Persistence.** Registry сбрасывает состояние в `$STATE/termbridge/registry.json` (версия, consent, capability overrides). Это позволяет восстановиться после крэша и показать текущее состояние в `agentcall status`.
- **Telemetry.** Каждое обновление эмитит span `termbridge.discovery.sync` и метрику `termbridge.capabilities.discovered{terminal,source}`; dashboards отслеживают прирост терминалов и дельты. Эти метрики закрывают задачи `task-termbridge-discovery-registry` и `task-termbridge-core-telemetry`.

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
| Тест-набор совместимости | **План.** `task-termbridge-test-suite` создаёт матрицу (containerised). | Регрессии ловятся автоматически. | Понадобятся эмуляторы или headless режимы терминалов.

### Security & Policy
- Каждая операция проходит через Rego policy (`data.shelldone.termbridge.allow`). Вход: команда, persona, terminal, capability flags, риск-метки.
- Опасные команды (spawn with remote command, broadcast paste) требуют `approval_granted`. Persona Nova получает guided overlay.
- Remote control API (iTerm2/kitty) по умолчанию disabled. Для enable → пользователь включает toggle, TermBridge фиксирует `consent` флаг в CapabilityMap, Logging: `termbridge.consent`.
- Action logging: `EventRecord::new("termbridge.action", …)` хранит `binding_id`, `command`, `args`, latency. Для ошибок — `termbridge.error`.
- ANSI sanitization: любые входящие строки проходят фильтр прежде чем попасть в Continuum/логи.

### Current Working Directory Sync
- **Domain contract.** `CurrentWorkingDirectory` (immutable value object) enforces length ≤4096, no control characters, valid path components. Aggregates expose `TerminalBinding::set_cwd` and `TerminalBinding::cwd()` to keep label updates explicit.
- **Application service.** `TermBridgeService::update_cwd` loads the binding, applies the value object, persists via `TerminalBindingRepository`, and emits Prism metrics (`termbridge.update_cwd`, `termbridge.error`). Missing bindings raise `TermBridgeServiceError::NotFound` → HTTP 404.
- **Interface.** `POST /termbridge/cwd` accepts `{binding_id, cwd}`; input is policy-gated с действием `cwd.update`, persona контекстом, terminal id и журналируется как `termbridge.cwd_update`. Denials → `termbridge.cwd_update.denied` с пояснениями Rego. `POST /termbridge/focus` принимает `{binding_id}` и инициирует перевод фокуса; policy проверяет действие `focus`, успех приводит к событию `termbridge.focus`, отказ фиксируется как `termbridge.focus.denied`.
- **Policy.** Default Rego adds `cwd.update` to the allowlist and limits path length; failures surface through `termbridge_deny_reason` including offending `cwd` for telemetry triage.
- **Verification.** Regression coverage lives in `termbridge` unit tests (`CurrentWorkingDirectory`, `TerminalBinding::set_cwd`, service update) and policy tests (`policy_termbridge_allows_cwd_update`, `policy_termbridge_denies_oversized_cwd_update`). Run `cargo test -p shelldone-agentd termbridge` and `opa eval` via `make verify` gates.

### UX Patterns
- **Context palette:** операции `Open here in <Terminal>`, `Send text to <Pane>`, `Duplicate layout`, `Attach clipboard`. Пункты отображаются только если capability `supported=true`.
- **CWD sync:** shell hooks (bash/zsh/fish/pwsh/cmd) из `scripts/shell-hooks/` отправляют OSC 7/9;9; CwdSyncSvc обновляет Continuum и UI (breadcrumbs). Фокусировка панелей (`/termbridge/focus`) синхронизируется с UI, чтобы палитра не предлагала неактивные binding’и.
- **Paste guard:** при мультилайн/невидимых символах → overlay (Nova/Core), bracketed paste всегда соблюдается, whitespace нормализуется, optional confirm.
- **Clipboard:** default OSC 52, fallback Wayland/X11/SSH/tmux; при превышении 75 KB — batching, user feedback.

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

# Security, Secrets, and Access Control

## Principles
1. **Least privilege:** every plugin, agent, or subsystem receives only the capabilities it needs.
2. **Transparency:** privileged actions are logged and auditable.
3. **Isolation:** untrusted components run in sandboxes (WASM, namespaces, containers).

## Threat Model
- Compromised plugin or extension.
- Leakage of keys and tokens (SSH, API credentials, agent certificates).
- Privilege escalation through syscalls, IPC, or unsafe command execution.
- Unauthorised agent access to user data.
- Misconfigured transports (gRPC/Σ-json) leading to agent impersonation or MITM.

## Secret Storage
- Primary mechanism: OS keyring (macOS Keychain, Windows Credential Vault, Secret Service).
- Additional layer `secrets/` (JSON + AES-GCM with a key sourced from the keyring) storing references and metadata.
- CLI commands: `shelldone secret add/list/revoke`, with expiration/rotation policies.

## Access Control
- `config/policies/*.yaml` define allowed actions (filesystem, network, shell commands).
- Agents rely on workflow approvals (manual + policy-based) with logs in `logs/agents.log`.
- RBAC: roles (owner/maintainer/contributor/viewer) mapped to capability sets (manage plugins, start agents, access UI areas).
- mTLS enforcement (gRPC):
  - `--grpc-tls-cert/--grpc-tls-key` — серверный сертификат/ключ.
  - `--grpc-tls-ca` — включить обязательную проверку клиентских сертификатов.
  - `--grpc-tls-policy` — политика шифрования (`strict`, `balanced`, `legacy`), влияет на набор протоколов и cipher suites (см. `docs/architecture/ai-integration.md`).
  - PEM-файлы мониторятся вотчером; перезапись любого файла приводит к автоматической перезагрузке без downtime, падаем в лог `agent.tls` при ошибке.
  - Встроенная команда `shelldone agent tls status` (roadmap) будет печатать текущие fingerprints; до её появления fingerprint публикуется в discovery и verify отчёте.
  - Discovery публикует hash используемого cert для агентов.
- Σ-json auth (roadmap): Noise/JWT; временно ограничено loopback + ephemeral tokens.

### TermBridge Policy & Consent
- Все команды TermBridge проходят Rego правило `data.shelldone.termbridge.allow`. Политика учитывает тип терминала, persona, capability flags (например, `requires_opt_in`, `supports_remote_exec`).
- iTerm2 API, kitty remote-control, D-Bus Konsole/Tilix выключены по умолчанию. Включение происходит через UX toggle → событие `termbridge.consent` + запись в CapabilityMap. Без consent команды `termbridge.*` возвращают `policy_denied`.
- Логи действий (`termbridge.action`) содержат `binding_id`, `command`, `args`, `latency`. В Continuum хранится audit trail (привязка к пользователю/персоне).
- Все входящие/исходящие строки очищаются ANSI-санитайзером, прежде чем попасть в Continuum/логи, предотвращая избежание alert’ов через ESC.
- PasteGuard (см. `termbridge.md`) блокирует вставки с подозрительными символами (ZWSP/NBSP) и требует подтверждение. Пороговые значения регулируются persona preset’ами.
- Spawn-пайплайн требует, чтобы capability map подтверждала `spawn=true`; при отсутствии consent или запрете политики запрос `POST /termbridge/spawn` отвечает `policy_denied`. Для wezterm токен pane_id сохраняется в binding и журнале, что исключает «слепую» отправку команд.
- Override `SHELLDONE_TERMBRIDGE_WEZTERM_CLI` проходит проверку существования/прав доступа: успешный override журналируется как `termbridge.capability.update{change="updated"}` (note `using override`), провал → `reason=not_supported` без silent fallback.
- `/termbridge/discover` допускает анонимный доступ только по loopback. Установка `SHELLDONE_TERMBRIDGE_DISCOVERY_TOKEN` (agentd) включает Bearer-аутентификацию; GUI и внешние клиенты должны передавать тот же токен через `SHELLDONE_AGENTD_DISCOVERY_TOKEN`. Дополнительная переменная `SHELLDONE_GUI_ALLOW_INSECURE_AGENTD` включает HTTP fallback и помечена как временная диагностика (production=HTTPS-only).

#### Enablement Checklist (per terminal)

| Terminal | User Action (performed once) | Shelldone Command | Audit Signal |
|----------|-----------------------------|-------------------|--------------|
| kitty | `kitty +kitten listen_on unix:/tmp/kitty` или `kitty --listen-on unix:/tmp/kitty` | `shelldone termbridge enable kitty --socket unix:/tmp/kitty` | `termbridge.consent{terminal="kitty"}` |
| WezTerm | CLI доступен из коробки; optional: `wezterm cli ls` sanity check | `shelldone termbridge enable wezterm` (валидирует CLI/pipe) | `termbridge.consent{terminal="wezterm"}` |
| iTerm2 | Preferences → General → «Enable Scripting» + Python API | `shelldone termbridge enable iterm2 --api-token ~/.config/iterm2/api.json` | `termbridge.consent{terminal="iterm2"}` |
| Windows Terminal | `settings.json` → `"experimental.featureFlags": { "advancedInput": true }` (если требуется), power user → `wt.exe` доступен | `shelldone termbridge enable wt` | `termbridge.consent{terminal="wt"}` |
| Konsole | `qdbus org.kde.konsole /Konsole` должен отвечать | `shelldone termbridge enable konsole --scope window` | `termbridge.consent{terminal="konsole"}` |
| Tilix | `sudo apt install tilix` + `tilix --session` | `shelldone termbridge enable tilix --session ~/.config/tilix/termbridge.json` | `termbridge.consent{terminal="tilix"}` |

`shelldone termbridge enable` выполняет безопасный handshake, записывает consent (TTL=30 дней) и предлагает инструкции (TL;DR карточка с ссылками на официальную документацию). Команда idempotent; повторное выполнение обновляет timestamp. Audit trail хранится 180 дней.

### TLS Operational Runbook

| Scenario | Command / Action | Expected Outcome | Escalation |
|----------|------------------|------------------|------------|
| Просмотр статуса | `scripts/tls/manage.sh status` | Печать SHA-256 fingerprint, сроки годности, текущая политика (`strict|balanced|legacy`). | Несовпадение с discovery → инцидент `SEC::TLS`. |
| Выпустить локальный cert | `scripts/tls/manage.sh issue --cn shelldone.local --out state/tls` | Созданы `cert.pem`, `key.pem`, `ca.pem` с правами 600/640. | Промышленные сертификаты: запрос PKI owner, ADR с TTL. |
| Горячая ротация cert/key | `scripts/tls/manage.sh rotate --cert cert.new.pem --key key.new.pem` | ≤5 с, метрика `agent.tls.reloads{result="success"}` увеличилась. | При `agent.tls.reload_errors` → откат и SEC incident. |
| Ротация CA | `scripts/tls/manage.sh rotate --cert cert.new.pem --key key.new.pem --ca ca.new.pem` | discovery публикует новый hash, старые клиенты получают `UNAUTHENTICATED`. | Если нужны параллельные CA → ADR и временный dual-stack. |
| SLA на reload | `grpcurl --cert client.pem --key client.key --cacert ca.pem localhost:17718 list` после перезаписи | Ответ 200, latency <5 с. | >5 с → создать issue `SEC-RELOAD-LATENCY`. |

**Failure Playbook**
1. Восстановить предыдущие PEM из `state/tls/backups/<timestamp>/`.  
2. Проверить gRPC доступность (`grpcurl … list`).  
3. Задокументировать событие в Continuum: `shelldone agent journal emit --kind tls.rollback --payload '{"reason":"reload_failure"}'`.  
4. В течение 1 ч провести post-mortem (темплейт `docs/security/runbook.md#tls`).

## Plugins and Sandbox
- Rust plugins requiring elevated privileges must declare `trusted=true` and origin metadata.
- WASM plugins are sandboxed by default (no FS/network) and gain capabilities via host API grants.
- Resource limits: cgroups (Linux), job objects (Windows), `sandbox-exec` (macOS).

## Incident Response
- Signature verification for plugins/updates (ed25519).
- Revocation list distributed with updates.
- Response playbook lives in `docs/security/runbook.md`; include security-team contacts.
- Sigma guard spool (`CACHE_DIR/sigma_guard_spool.jsonl`) сохраняет отчеты при недоступности agentd.
  - Env: `SHELLDONE_SIGMA_SPOOL=0` отключает, `SHELLDONE_SIGMA_SPOOL_MAX_BYTES` задаёт предел.
  - При восстановлении соединения спул воспроизводится и журналируется.

## Checks
- Static validation of policy/configs via `python3 scripts/verify.py` (extend `scripts/verify.py`).
- Secret scanning (git-secrets or custom check) in `python3 scripts/verify.py --mode full`.
- Containerised smoke tests in `python3 scripts/verify.py --mode full` (WASM sandbox + capability drops).

## Milestones
1. ADR for the secret/key-management model.
2. Implement secret manager API + CLI.
3. Enforce policy files at runtime.
4. Sandbox plugins with resource limits (aligned with perf budgets).
5. Finalise the incident runbook and revocation processes.

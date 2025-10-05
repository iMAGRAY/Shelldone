# Agent Governance and Operations

## Overview
This specification defines how Shelldone manages third-party agent SDK adapters (OpenAI, Claude, Microsoft), secrets, lifecycle, observability, and developer tooling. It complements `docs/architecture/ai-integration.md` and UTIF-Σ.

## Adapter Lifecycle
1. **Registration**: adapters declared in `agents/manifest.json` with metadata (id, description, command, error_contains, version, signature).
2. **Installation**: `make agents-install` (TODO) bootstraps required runtimes (Python venv, npm ci) and verifies lock files.
3. **Smoke Verification**: `python3 scripts/agentd.py smoke` executed in `make verify-prepush`.
4. **Health Monitoring**: `shelldone-agentd` polls adapters via readiness handshake (`status: ready`). Failures emit `sigma.adapter.unhealthy` events.
5. **Upgrade/Rollback**:
   - Upgrades performed via `shelldone agent adapters update <id>` (future CLI).
   - Lock files signed (Ed25519). On failure, revert to previous signature snapshot.
   - `agents/REVOCATIONS.md` lists banned versions.

## Secrets Management
- All agent adapters require API keys (`OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `MICROSOFT_AGENT_API_KEY`).
- Keys are stored using Secret Manager service (`shelldone secrets`) with escrow to OS keyring + encrypted overlay.
- `scripts/agentd.py` loads secrets via `shelldone secrets export --adapter <id>`; environment injection done per process invocation.
- Secret rotation tracked in `state/secrets/ledger.json` with timestamp and owner.
- Policies enforce least privilege: adapters cannot read other secrets.

## Runtime Service (`shelldone-agentd`)
- Always-on daemon стартует через `make run-agentd` (roadmap `shelldone agent serve`).
- MCP транспорты: WebSocket + gRPC (TLS/mTLS). Discovery-файл `~/.config/shelldone/agentd.json` **roadmap (DOC-01)** — см. pain matrix.
- Heartbeat в адаптеры; автоперезапуск с экспоненциальным backoff.
- Health endpoints (current): `/healthz`, `/journal/event`. Roadmap (OBS-04) добавит `/status`, `/context/full`.
- Состояние сессий хранится в `state/mcp_sessions.json` (атомарный JSON), Continuum журнал — `state/journal/`.
- TLS-контур: `--grpc-tls-policy <strict|balanced|legacy>` ограничивает поддерживаемые протоколы и cipher suites; PEM-файлы (`--grpc-tls-cert/--grpc-tls-key/--grpc-tls-ca`) вотчатся и перезагружаются ≤5 c без простоя, ошибки пишутся в `logs/agents.log` и метрику `agent.tls.reload_errors`.

## UTIF-Σ Integration
- Σ-cap handshake caches adapter capabilities (`adapter://openai`, etc.).
- CLI `shelldone agent <cmd>` uses HTTP endpoints and handles fallback.
- Σ-pty proxy logs `sigma.proxy.disabled` when agentd unreachable (see ADR-0005).
- TLS-контур: `--grpc-tls-policy <strict|balanced|legacy>` ограничивает поддерживаемые протоколы и cipher suites; PEM-файлы (`--grpc-tls-cert/--grpc-tls-key/--grpc-tls-ca`) вотчатся и перезагружаются ≤5 s без простоя, ошибки пишутся в `logs/agents.log` и метрику `agent.tls.reload_errors`.
- TermBridge Orchestrator (см. `termbridge.md`) управляет внешними терминалами и публикует Capability Map. CLI получает команды `shelldone termbridge <...>` (roadmap), MCP — `termbridge.capabilities/list/focus/send_text`. Governance отвечает за контроль соответствия политике (consent, audit, toggles).

## Observability
- Metrics exported via OTLP: `agent.adapter.ready{adapter=}`, `agent.exec.latency`, `agent.exec.errors`, `agent.journal.accepted`, `agent.policy.denials`, `sigma.guard.events`.
- Logs: `logs/agents.log` with structured entries (adapter, status, message).
- Alerts: adapter unhealthy >2 min, secret rotation overdue (>30 days), exec error rate >5%.

### Operator Playbook (excerpt)

| Situation | Command | Expected result | Notes |
|-----------|---------|-----------------|-------|
| Обновить discovery | `shelldone agent discovery --write ~/.config/shelldone/agentd.json` | JSON содержит `grpc_tls_policy`, fingerprints, policy hash. | запускать после любой ротации TLS/policy. |
| Проверить адаптер | `shelldone agent adapters status` | `ready` для всех зарегистрированных (OpenAI/Claude/Microsoft). | fallback: `python3 scripts/agentd.py smoke`. |
| Перезапустить agentd | `systemctl --user restart shelldone-agentd` (roadmap) | gRPC + Σ-json поднимаются, Continuum журнал сохраняется. | до появления systemd — `make run-agentd`. |
| Очистить Continuum spool | `rm -f $CACHE_DIR/sigma_guard_spool.jsonl` | Следующий flush создаст новый файл, события сохранятся. | Делать только после подтверждённого recovery. |
| Заблокировать auto-update `agentcall` | `AGENTCONTROL_AUTO_UPDATE=0 agentcall <cmd>` | Команда выполняется без повторного обновления CLI. | Версия 0.3.1 зафиксирована в `agentcontrol/state/tooling.lock`; экспортируйте переменную в shell или profile. |

Воркфлоу “инцидент безопасности” → см. `docs/security/runbook.md`. Каждое действие агент публикует в Continuum (`agent.journal`) для воспроизводимости.

## Testing Strategy
- **Smoke**: runtime presence (already in `scripts/agentd.py`).
- **Contract**: add `tests/agentd_contract.rs` (TODO) verifying `/sigma/handshake`, `/ack/exec`, `/journal/event` across adapters (mocked).
- **Integration**: `make verify-full` spins up agentd + adapters in container, executes sample requests, records Continuum events.
- **Security**: secret scanner ensures API keys not checked into repo; `npm audit`/`pip audit` run in CI.

## Developer API
- Rust crate `shelldone-agent-api` (WIP) предоставляет typed client (`initialize/list/call`, context dump, playbooks).
- JS CLI: `shelldone agent <cmd>` — интерактивные утилиты, автогенерация TLS.
- Recipes: `docs/recipes/agents.md` покрывает onboarding, playbooks, remediation.

## Roadmap
1. Implement autostart/service management for agentd.
2. Build secret manager CLI integration for adapters.
3. Add contract tests & CI containers for adapters.
4. Publish developer API crate and docs.
5. Integrate adapter health metrics into Prism dashboards.

## Related ADRs
- ADR-0001 UTIF-Σ Control Plane
- ADR-0002 ACK Kernel
- ADR-0003 Persona Engine
- ADR-0004 Capability Marketplace Hooks
- ADR-0005 Σ-pty Proxy Integration
- ADR-0006 Microsoft Agent SDK Adapter

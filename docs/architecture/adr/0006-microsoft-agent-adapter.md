# ADR 0006: Microsoft Agent SDK Adapter

- **Status:** Proposed (TTL 2025-10-24)
- **Context:** Требуется parity для Microsoft Agent SDK наряду с OpenAI и Claude, включая journal/logging и capability negotiation через UTIF-Σ.
- **Decision:**
  - Добавить `agents/microsoft` с Node.js мостом (`@microsoft/agents-sdk`), описать зависимости и smoke-тесты.
  - Обновить `agents/manifest.json`, `scripts/agentd.py` и документацию для поддержки Microsoft адаптера.
  - Требование: переменная `MICROSOFT_AGENT_API_KEY`, fallback-friendly ошибки для smoke.
- **Consequences:**
  - Увеличивается нагрузка на maintenance (npm lock, security scanning).
  - Появляется новая поверхность секретов — необходимо расширить policy guardrails и secret manager.
- **Rollback Plan:** git tag `codex/2025-10-03-pre-ms-agent`, удалить `agents/microsoft` и записи из manifest, пересобрать `scripts/agentd.py`.
- **Testing:** smoke (`python3 scripts/agentd.py smoke`), contract tests в `shelldone-agentd` (ready/error), security scanning npm lock.
- **Owners:** AI Automation squad.
- **Dependencies:** ADR-0001, ADR-0002.

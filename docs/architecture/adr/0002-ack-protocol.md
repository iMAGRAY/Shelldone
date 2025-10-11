# ADR 0002: ACK (Agent Command Kernel)

- **Status:** Accepted (2025-10-03)
- **Context:** Агентам и людям нужна минимальная, детерминированная команда из 8 операций поверх UTIF-Σ для симметричных сценариев.
- **Decision:**
  - Закрепить команды `agent.plan|exec|form|undo|guard|journal|inspect|connect` с обязательными spectral-тегами.
  - Все новые автоматизации/SDK используют ACK вместо произвольных CLI RPC.
  - Persona/Policy движки интегрированы через `agent.guard`.
- **Consequences:**
  - Требуется обновить SDK/CLI для поддержки ACK.
  - Возникают строгие контракты и тесты на обратимость.
  - Снижается площадь API и упрощается контроль безопасности.
- **Rollback Plan:** git tag `codex/2025-10-03-pre-ack`, скрипт `./scripts/rollback-ack.sh` отключает ACK, возвращает старые команды.
- **Testing:** contract tests на ACK → Σ-pty, persona switching, denial матрицы (Rego).
- **Owners:** imagray `<magraytlinov@gmail.com>` (support: AI Automation squad).
- **Dependencies:** ADR-0001.

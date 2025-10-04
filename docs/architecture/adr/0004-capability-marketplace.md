# ADR 0004: Capability Marketplace Hooks

- **Status:** Accepted (2025-10-03)
- **Context:** Партнёры/сообщество нуждаются в безопасном способе доставки capability bundles. Требуется интеграция marketplace с Σ-cap и политиками.
- **Decision:**
  - Marketplace пакеты описываются манифестом (`capabilities.yaml`) и проходят Σ-cap валидацию до установки.
  - Policies (Rego) и security review блокируют неподписанные или опасные capabilities.
  - В `plugins/registry/` появляется Hook API для marketplace lifecycle.
- **Consequences:**
  - Дополнительные проверки при установке, потенциальный рост времени установки.
  - Появляется единая точка обновлений и отзывов capabilities.
- **Rollback Plan:** git tag `codex/2025-10-03-pre-marketplace`, отключение marketplace через `shelldone marketplace disable` и policy revert.
- **Testing:** контрактные тесты установки/отката, security fuzz на манифесты, perf измерения handshake.
- **Owners:** Plugin Platform squad.
- **Dependencies:** ADR-0001, ADR-0002.

# ADR 0001: Введение UTIF-Σ Control Plane

- **Status:** Accepted (2025-10-03)
- **Context:** Требуется единая шина управления терминалом и агентами с гарантированными SLA, безопасностью и совместимостью (см. `docs/architecture/utif-sigma.md`).
- **Decision:**
  - Принять UTIF-Σ (Σ-pty, Σ-json, Σ-cap) как стандартный control plane.
  - Обязать все новые протоколы/плагины использовать ACK и Σ-cap handshake.
  - Ввести Continuum snapshots и Prism telemetry как обязательные части `make verify-full`.
- **Consequences:**
  - Появляется обязательный PTY proxy слой; legacy режим поддерживается через downgrade.
  - Требуются perf и security проверки каждого релиза.
  - Упрощается интеграция агентов, сокращается когнитивная нагрузка.
- **Rollback Plan:** git tag `codex/2025-10-03-pre-utif`, CLI `./scripts/rollback-utif.sh` (удалить Σ-* слои, вернуть прямой PTY).
- **Testing:** k6 perf (`scripts/perf/utif_exec.js`), property tests (`replay == state`), security fuzz для ESC.
- **Owners:** Platform Resilience squad.
- **Dependencies:** Completion of `task-utif-sigma-foundation`.

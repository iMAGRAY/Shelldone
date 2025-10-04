# ADR 0005: Σ-pty Proxy Integration

- **Status:** Proposed (TTL 2025-10-24)
- **Context:** UTIF-Σ требует, чтобы терминал делегировал PTY события через защищённый прокси, обеспечивающий escape-фильтрацию, capability downgrades и Continuum-журналирование.
- **Decision:**
  - Внедрить Σ-pty proxy как обязательный слой между mux и реальными PTY (локальными, ssh, wsl).
  - Все выводимые OSC/CSI проходят через sandbox allowlist; нарушения проксируются в policy `agent.guard`.
  - Proxy публикует события в `shelldone-agentd /journal/event` и поддерживает downgrade на legacy режим при недоступности сервиса.
- **Consequences:**
  - Требуется переработка модулей `mux`, `shelldone-client`, `portable-pty` glue.
  - Нужно гарантировать задержку < 3 мс и отсутствие регрессий в фреймере.
  - Появляется зависимость от `shelldone-agentd`; необходим встроенный fallback.
- **Rollback Plan:** git tag `codex/2025-10-03-pre-sigma-pty`, переменная `SHELLDONE_SIGMA_PTY=0` отключает прокси, возвращая прямое подключение PTY.
- **Testing:** contract tests (escape allowlist, downgrade), perf k6 (`scripts/perf/utif_exec.js` + новый Σ-pty benchmark), fuzz ESC parser, integration tests через `make verify-full`.
- **Owners:** Platform Resilience squad.
- **Dependencies:** ADR-0001, ADR-0002.

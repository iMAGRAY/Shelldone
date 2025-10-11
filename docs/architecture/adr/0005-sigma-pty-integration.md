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
- **Testing:** contract tests (escape allowlist, downgrade), perf k6 (`scripts/perf/utif_exec.js` + новый Σ-pty benchmark), fuzz ESC parser, integration tests через `python3 scripts/verify.py --mode full`.
- **Owners:** imagray `<magraytlinov@gmail.com>` (support: Platform Resilience squad).
- **Dependencies:** ADR-0001, ADR-0002.

## Acceptance Criteria (RTF-aligned)
- Escape/OSC allowlist строго применяется; нарушения → `agent.guard` + событие `sigma.guard` (count ≥1 при негативных тестах).
- Σ-pty overhead ≤ 3 мс p95 (перф профиль `utif_pty`).
- Fallback legible: при недоступности agentd автоматически активируется legacy путь; логируется `sigma.proxy.disabled` (1× на сессию).
- Downgrade события `sigma.downgrade` эмитятся при capability mismatch (tmux/ConPTY и т.п.).
- OTLP метрики доступны: `shelldone.sigma.guard.events{reason, direction}`.

## Feature Flags & Rollout
- `SHELLDONE_SIGMA_PTY=0` — принудительное отключение proxy (legacy direct PTY).
- `SHELLDONE_SIGMA_SPOOL_MAX_BYTES` — предел буфера событий; по умолчанию 1 MiB.
- Rollout: canary 10% с авто-откатом при росте `sigma.guard` событий >X3 от базовой линии 24 ч.

## Risks & Mitigations (дополнение)
- Перф регрессия → включить streaming‑mode fallback, поднять sampling perf до 1/100 оп.
- Совместимость терминалов → Capability Map enforced, для несовместимых — downgrade и UI-toast.

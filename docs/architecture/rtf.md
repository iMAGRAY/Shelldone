# Ready-to-Field (RTF) Gate — Shelldone Q4 2025

> Версия: 2025-10-09 · Статус: Proposed (pending owner sign-off) · Владелец: imagray <magraytlinov@gmail.com>

Этот документ определяет единый «готов к эксплуатации» (RTF) гейт. RTF дополняет существующие Ready-for-Test (RFT) ворота из `docs/architecture/manifest.md`, фокусируясь на эксплуатационной готовности: безопасность, наблюдаемость, производительность в бюджете, отказоустойчивость, откат и операционные процедуры.

## 1. Нормативные источники
- Архитектурный манифест: `docs/architecture/manifest.md`
- Производственные бюджеты: `docs/architecture/perf-budget.md`, разделы UTIF-Σ (`docs/architecture/utif-sigma.md`)
- Безопасность и секреты: `docs/architecture/security-and-secrets.md`, `docs/security/runbook.md`
- Наблюдаемость: `docs/architecture/observability.md`
- Релиз и совместимость: `docs/architecture/release-and-compatibility.md`

## 2. Область RTF
RTF покрывает минимально достаточные подсистемы MVP (см. `docs/ROADMAP/MVP.md`): Sigma Guard/Σ-pty proxy, TermBridge, Agent Bridge (ACK), Continuum & Observability, Plugin SDK (preview).

## 3. Gate чек-лист (все пункты обязательны)

### 3.1 Функциональные и контрактные инварианты
- ADR-0001 и ADR-0002 — статус «Accepted»; ADR-0005 — «Accepted» либо за флагом `SHELLDONE_SIGMA_PTY=0` (legacy fallback разрешён).
- ACK минимальный набор команд: `agent.exec|plan|guard|journal` — интеграционные тесты зелёные (см. `shelldone-agentd/tests/`).

### 3.2 Производительность (порог p95, иначе FAIL)
- UTIF-Σ:
  - Σ-cap handshake round-trip ≤ 5 ms (p95).
  - ACK overhead (без времени команды) ≤ 3 ms (p95).
  - Continuum append ≤ 1 ms (p95).
- UX/TTI: Time-to-Interactive ≤ 20 ms; переключение вкладок ≤ 80 ms.
- TermBridge: `spawn` ≤ 250 ms (p95) на эталонном ноутбуке (см. perf-budget).

Источник проверки: `artifacts/perf/**` и k6-скрипты из `scripts/perf/`; прогон обязателен под `make verify`.

### 3.3 Надёжность и деградации
- Backpressure в TermBridge активен; перегрузка сливается ≤ 1 с после простоя.
- Σ-pty fallback: при недоступности agentd — автоматический downgrade с событием `sigma.proxy.disabled` (однократно на сессию).

### 3.4 Безопасность (SCA/лицензии/секреты)
- `cargo deny`/OSV/pip-audit: 0 High/Critical; отчёт `reports/security.json` приложен.
- Политики Rego загружены; `policy.explain` покрыт контракт-тестами; Forbidden markers baseline — без новых нарушений.

### 3.5 Наблюдаемость
- Метрики доступны: `shelldone.sigma.guard.events`, `termbridge.actions_total`, `agent.heartbeat.age_ms`, `continuum.snapshot.latency_ms`.
- Трассировки OTLP с `binding_id/terminal_id/persona` (Baggage) присутствуют в e2e прогоне.

### 3.6 Операционность и откат
- Runbooks: TLS/секьюрити и инциденты — актуальны (`docs/security/runbook.md`, `security-and-secrets.md`).
- Откат: тег релиза и фича-флаги документированы; минимально: `SHELLDONE_SIGMA_PTY=0` (disable proxy), `TERM_BRIDGE_BACKPRESSURE=off`.

## 4. Процедуры проверки (выполнять в указанном порядке)
1. `VERIFY_MODE=prepush make verify` — быстрый гейт (fmt/lint/tests/SCA/perf smoke).
2. `make review` — дифф-ориентированный QA, проверка evidence/артефактов.
3. `make ship` — релизный гейт: lock deps, SBOM, SCA, публикация отчётов.

Сформированные артефакты (минимум):
- `reports/verify.json`, `artifacts/perf/*.json`, `reports/security.json`, `reports/agents/mcp-demo.log`.

## 5. Выходные критерии (RTF = PASS)
- Все пункты 3.1–3.6 выполнены; perf пиков p95 в пределах порогов; 0 High/Critical в SCA; OTLP трассировки и ключевые метрики присутствуют; оформлен откат.

## 6. Владение и lifecycle
- Владелец RTF-гейта: imagray (Docs/QA ко-владельцы по разделам).
- Ревизия раз в квартал или при существенном изменении перф/политик.

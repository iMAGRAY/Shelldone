# Shelldone MVP — Q4 2025

> Версия: 2025-10-09 · Статус: Proposed (pending owner sign-off) · Владелец: imagray <magraytlinov@gmail.com>

Этот документ фиксирует единое определение MVP платформы на Q4 2025 и проверяемые Acceptance Criteria (AC). Он агрегирует цели из `docs/ROADMAP/2025Q4.md` и архитектурных спецификаций.

## 1. Границы MVP
Включено (минимально достаточное):
- Sigma Guard / Σ-pty proxy (под флагом допускается legacy fallback).
- TermBridge: `spawn|focus|send_text|clipboard.write` + Capability Map snapshot.
- Agent Bridge (ACK): `agent.exec|plan|guard|journal` через MCP WebSocket/gRPC/STDIO.
- Continuum & Observability: журнал событий, базовые OTLP метрики и трассировки.
- Plugin SDK (preview): `cargo check` + 2 примера (сборка и doctest’ы).

Исключено: полноценный Marketplace UI, коллаборативные сессии, продвинутые графические эффекты GA.

## 2. Acceptance Criteria (проверяемые)
| Блок | AC | Проверка |
| --- | --- | --- |
| Σ-pty | Allowlist ESC/OSC; событие `sigma.downgrade` при мисматче; fallback включается переменной | Интегр. тесты + k6 `utif_pty`; лог в `/journal/event` |
| TermBridge | 3 терминала (WezTerm, Kitty, Windows Terminal) обнаруживаются; `spawn` p95 ≤ 250 мс | `scripts/tests/termbridge_matrix.py`, perf артефакты |
| ACK | Команды `exec/plan/guard/journal` проходят e2e; error rate < 1% | e2e тесты и трассировки |
| Continuum | Append ≤ 1 мс p95; восстановление ≤ 150 мс | perf/k6 и smoke восстановление |
| Obs. | Метрики `sigma.*`, `termbridge.*`, `agent.*` поступают; кор. трассы есть | `scripts/tests/check_otlp_payload.py` |
| Security | 0 High/Critical (SCA); секреты — 0 | `reports/security.json` |

## 3. Команды для приёмки
```
VERIFY_MODE=prepush make verify
make review
make ship
```

## 4. График
- MVP Cut: 2025-11-30 (совпадает с Plugin Platform MVP в манифесте).

## 5. Риски и Plan B
- Σ-pty регрессии → временный legacy режим через `SHELLDONE_SIGMA_PTY=0`.
- Неполная поддержка терминалов → ограничение матрицы, видимая деградация без падения RTF.

## 6. Трассировка связей
- Roadmap: `docs/ROADMAP/2025Q4.md`
- Манифест: `docs/architecture/manifest.md`
- RTF: `docs/architecture/rtf.md`

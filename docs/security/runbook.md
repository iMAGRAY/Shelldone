# Shelldone Security Runbook

Версия: 2025-10-09 · Статус: In use · Владелец: security@shelldone.dev · DRI: imagray

## 1. Цели и охват
- Быстрая реакция на инциденты (σ‑pty/TermBridge/Agent Bridge/TLS/policy).
- Минимизация ущерба, сохранность артефактов и трассируемость.
- Интеграция с RTF: все шаги совместимы с `docs/architecture/rtf.md`.

## 2. Контакты и каналы
- DRI (дежурный): imagray `<magraytlinov@gmail.com>`
- Канал оповещений: `#security-incidents` (внутренний)
- Отчётность: `reports/incidents/<YYYY-MM-DD>/<id>/` (см. секцию 6)

## 3. Классификация и SLO реакции
| Sev | Пример | SLO первичной реакции | Цель восстановления |
| --- | --- | --- | --- |
| SEV-1 | Утечка секретов, RCE цепочка | ≤15 мин | Митигация ≤1 ч, RCA ≤24 ч |
| SEV-2 | Критический отказ TermBridge/ACK | ≤30 мин | Восстановление ≤2 ч |
| SEV-3 | Деградация перф/наблюдаемости | ≤2 ч | Восстановление ≤24 ч |

## 4. Общая процедура TRIAGE (10 шагов)
1) Freeze: остановить релизы/фичи с риском (флаги ниже).
2) Захват артефактов: `reports/verify.json`, `reports/security.json`, `artifacts/perf/*`, журналы `/journal/event` (JSONL) и OTLP срез.
3) Идентификация компонента: σ‑pty / TermBridge / ACK / TLS / Policy.
4) Включить расширенные логи (env): `SHELLDONE_LOG=trace`, `SHELLDONE_SIGMA_SPOOL_MAX_BYTES=4194304`.
5) Митигация через флаги:
   - `SHELLDONE_SIGMA_PTY=0` — отключить proxy, вернуть legacy PTY.
   - `TERM_BRIDGE_BACKPRESSURE=off` — временно снять backpressure (для диагностики, не для долгой эксплуатации).
6) Изоляция: отозвать внешние binding’и/ключи, отключить подозрительные агенты.
7) Policy: включить строгий режим deny‑by‑default, проверить `policy.explain` для последних отказов.
8) Коммуникация: SEV‑уровень, статус, ETA восстановления в канал инцидентов.
9) Вернуться к норме: включить откат фич (см. RTF), подтвердить метрики/трейсы.
10) Постмортем ≤24 ч: 5 Whys, action items, обновление документации.

## 5. Частные плейбуки
### 5.1 Σ‑pty / ESC‑инъекции
- Симптомы: всплеск `shelldone.sigma.guard.events`, некорректный вывод/отказ ACK.
- Действия: проверить allowlist, активировать legacy режим (`SHELLDONE_SIGMA_PTY=0`), собрать `utif_pty` perf, провести fuzz ESC.

### 5.2 TermBridge / IPC дрейф
- Симптомы: рост `termbridge.errors{reason=not_supported|timeout}`, падение обнаружения терминалов.
- Действия: зафиксировать Capability Map snapshot, проверить версии CLI, срез трейс‑дерева; временно сократить матрицу терминалов.

### 5.3 ACK / Агентские сбои
- Симптомы: error rate >1% в `agent.exec.errors`.
- Действия: проверить токены/политики, включить `agent.exec` трейс, зафризить marketplace‑агентов.

### 5.4 TLS / Комплаенс
- Симптомы: ошибки reload, отсутствие mTLS.
- Действия: запустить TLS Runbook (см. `security-and-secrets.md`), катить предыдущий ключевой набор.

## 6. Артефакты инцидентов (обязательно)
- `reports/incidents/<date>/<id>/context.json` — версия, флаги, окружение.
- `reports/incidents/<date>/<id>/metrics.prom` — срез ключевых метрик.
- `reports/incidents/<date>/<id>/trace.json` — экспорт трасс.
- `reports/incidents/<date>/<id>/timeline.md` — повременной лог действий.

## 7. Критерий закрытия
- Метрики восстановлены (σ‑pty/TermBridge/ACK); SCA 0 High/Critical; трассы и журналы полны; постмортем и действия зафиксированы.

Ссылки: `docs/architecture/rtf.md`, `docs/architecture/manifest.md`, `docs/architecture/observability.md`.

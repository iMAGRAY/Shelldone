# ADR 0007: Experience Hub God-Mode UI Overlay

- **Status:** Accepted (2025-10-05)
- **Context:** Persona Engine и агентные сценарии требуют единой "god-mode" консоли, которая отображает состояние рабочих пространств, агентов и UX намерения без выхода из терминала. Текущий UI не агрегирует эти сигналы, что ломает обещанный флоу для multi-agent orchestration.
- **Decision:**
  - Выделить bounded context `experience` (domain/app/ports/adapters) с агрегатом `ExperienceLayout` и сервисом `ExperienceOrchestrator`.
  - Ввести команду `ShowExperienceHub` (по умолчанию `SUPER+G`) с DDD-портом `ExperienceRendererPort` и адаптером `TerminalOverlayRenderer`.
  - Рендерить Experience Hub как overlay поверх активного таба, отображая метрики, агенты и persona-интент с 60 FPS бюджетом.
- **Consequences:**
  - Требуются дальнейшие интеграции с фактическими approval pipelines (TTL TEMP/EXPHUB/7d).
  - Необходимо расширить telemetry (`agent_activity`) и perf-baselines `experience_hub.wgsl` (follow-up task `task-experience-telemetry`).
- **Rollback Plan:** git tag `codex/2025-10-05-pre-experience-hub`; удалить `shelldone-gui/src/experience/**`, команду `ShowExperienceHub`, и re-run `python3 scripts/verify.py`.
- **Testing:** Юнит-тесты на агрегат, адаптер; `python3 scripts/verify.py` с diff coverage ≥ 90%.
- **Owners:** imagray `<magraytlinov@gmail.com>` (support: Experience squad · Animation/Persona chapter).
- **Dependencies:** ADR-0003 (Persona Engine), ADR-0004 (Capability Marketplace).

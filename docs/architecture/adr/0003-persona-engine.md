# ADR 0003: Persona Engine Nova/Core/Flux

- **Status:** Accepted (2025-10-03)
- **Context:** Пользователи и агенты требуют адаптивного UX, уменьшающего шум и перегруз. Требуется формализовать режимы Nova/Core/Flux и политику подсказок.
- **Decision:**
  - Ввести Persona Engine с профилями, описанными в `config/personas/*.yaml`, связанный с ACK и policy prompts.
  - Переключение персон — часть Σ-cap handshake; режимы управляют подсказками и guardrails.
  - UX исследования (SUS ≥ 85 для Nova) обязательны перед GA.
- **Consequences:**
  - Нужны UI/CLI обновления, тесты на шум подсказок.
  - Требуется дополнительный telemetry канал для persona метрик.
- **Rollback Plan:** git tag `codex/2025-10-03-pre-persona`, CLI `shelldone persona disable --all` + revert конфигов.
- **Testing:** UX сценарии, property tests на отсутствие подсказок в Flux, perf профили подсказок.
- **Owners:** Experience squad.
- **Dependencies:** ADR-0001, ADR-0002.

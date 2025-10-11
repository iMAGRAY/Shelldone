# Heart Sync Guardrail — 5 Oct 2025

- Legacy Memory Heart workflow больше не используется. Требуется новый инструмент синхронизации.
- `scripts/project_health_check.py` остаётся статическим анализатором: помогает обнаружить устаревшие статусы/документы.
- Политика: после крупных изменений документации обновляйте `docs/status.md`, `docs/tasks.yaml`, `docs/architecture/manifest.md`; при необходимости временно запускайте `python3 scripts/agentd.py heart --sync` (TODO) либо пересоздавайте индексы вручную.
- Автоматический heart-sync не планируется до появления нового оркестратора.

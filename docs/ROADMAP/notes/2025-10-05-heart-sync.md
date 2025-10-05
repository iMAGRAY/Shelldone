# Heart Sync Guardrail — 5 Oct 2025

- Reviewed Memory Heart workflow (`agentcontrol/scripts/agents/heart_engine.py`). The CLI exposes `sync/refresh/update` without status reporting; stale indices were invisible.
- Added `scripts/project_health_check.py` to compute roadmap progress warnings, detect task board age, and flag Heart indices older than 6 h.
- Policy: run `agentcall heart sync` after structure-heavy changes (docs, architecture, manifests) or immediately when the health check exits with non-zero status.
- No automatic Heart sync wired into `agentcall status`; manual trigger remains preferred to avoid long re-indexing during tight loops.

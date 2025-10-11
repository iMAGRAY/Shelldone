# Shelldone State Management and Resilience

## Goals
- Deterministic recovery of sessions, tabs, and user preferences.
- Correct behaviour across crashes, updates, and multi-machine usage.
- Clear policy for backups, sync, and schema migrations.

## Storage Components
- **Config store:** `$XDG_CONFIG_HOME/shelldone/` (configuration, themes, policies, Σ-cap manifests).
- **State store:**
  - `sessions/` — JSON descriptors plus binary snapshots for fast restore.
  - `journal/` — Continuum event log (`*.jsonl`) with spectral tags.
  - `snapshots/` — Merkle-indexed diffs (compressed) for ≤150 ms restore.
  - `cache/` — transient data (glyph atlases, build artefacts, telemetry buffers).
  - `logs/` — agent actions, audit trail, system logs.
- **Agent feed:** runtime публикует события через `shelldone-agentd /journal/event`; при недоступности сервиса логирует `sigma.proxy.disabled` в `logs/agents.log` и продолжает локально.
- **Secret vault:** encrypted store for tokens/keys (OS keyring + encrypted overlay in `secrets/`).

## Requirements
1. **Crash safety:** journaled writes for snapshots, atomic updates (write temp → fsync → rename).
2. **Schema versioning:** every format carries a version; migrations live in the `shelldone_migrations` crate.
3. **Cross-device sync:** `shelldone sync push/pull` (rsync/git/cloud) with selectable strategies (full vs partial) поверх Continuum diffs.
4. **Backups:** automatic snapshots every N hours (`state/backups/`), checksum + rotation, optional encryption (AES-GCM) + Ed25519 signatures.
5. **Telemetry opt-in:** anonymous metrics stored separately (`metrics/`) and purged when telemetry is disabled; Prism metadata cross-links события.

## Roadmap Integration
- Big task `task-state-persistence` covers implementation.
- `python3 scripts/verify.py` should lint serialisable models and run load/save smoke tests.

## State Sync UI (Experience Hub)
- Experience Hub overlay **State Sync** surfaces the latest workspace snapshots pulled
  from `state/snapshots/`. The panel shows recency, size, storage path, and
  classification tags (auto/manual/protected).
- Highlight ratio blends recency and snapshot density; fresh snapshots (<15 min)
  glow at full intensity while older sets degrade to 0.3.
- Callout tile summarises the newest snapshot and links to the on-disk path for
  quick inspection prior to restore.
- Global actions provide one-keystroke recovery flows: `Ctrl+Shift+R` restores the
  latest snapshot via `shelldone state restore`, `Ctrl+Shift+O` reveals the snapshots
  directory in the OS file manager, and `Ctrl+Shift+C` copies the snapshot path to
  the clipboard. Each action emits persistent toast notifications for success or
  failure (missing CLI, I/O errors, etc.), giving immediate feedback.
- Telemetry gauges: `experience.snapshots.count` and `experience.snapshots.recency`
  (future) allow dashboards to track backup coverage.
- Fallback states: when no snapshots exist the overlay renders guidance instead
  of an empty list, nudging operators to run `shelldone state save`.

## Next Steps
1. Create an ADR covering snapshot format (JSON vs SQLite vs binary).
2. Implement `shelldone state save/restore` with automatic recovery on startup.
3. Build the sync CLI/daemon.
4. Add a state health-check to `python3 scripts/verify.py --mode full` (selective round trip).
5. Write user-facing documentation (`docs/recipes/state-backup.md`).

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
- `make verify` should lint serialisable models and run load/save smoke tests.

## Next Steps
1. Create an ADR covering snapshot format (JSON vs SQLite vs binary).
2. Implement `shelldone state save/restore` with automatic recovery on startup.
3. Build the sync CLI/daemon.
4. Add a state health-check to `make verify-full` (selective round trip).
5. Write user-facing documentation (`docs/recipes/state-backup.md`).

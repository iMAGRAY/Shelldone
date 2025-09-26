# Shelldone Release, Updates, and Compatibility

## Release Engineering
- **Channels:** nightly → beta → stable. Each channel ships tarballs, deb/rpm packages, winget manifests, and a Homebrew cask.
- **Signing:** all artefacts are signed by the Maintainer Team (ed25519; cosign for containers). Public keys are checked into `release/keys/`.
- **Auto-updates:** built-in updater (`shelldone self update`) plus integrations with system package managers.
- **CI/CD:** `make ship` runs `make verify VERIFY_MODE=prepush`, bumps the version, refreshes the changelog, and publishes via GitHub Actions or the internal runner.

## Versioning and Compatibility
- **SemVer:** majors change on breaking APIs (plugins, config, agent protocol).
- **Config migrations:** `shelldone migrate` upgrades configuration and state. Migrations live in `migrations/` with round-trip tests.
- **Plugin API:** the `shelldone_api` crate uses `#[deprecated]` annotations and `compat_vN` feature flags. Deprecation notes live in `docs/plugins/compat.md`.
- **State snapshots:** capture schema versions; on incompatible upgrades we back up the previous state and run migrations.

## Delivery and Catalogues
- Plugin/theme catalogues stay in lockstep with the release cycle by declaring `sdk_version` in manifests.
- The `release/` branch stores manifest files (checksums, URLs, metadata).
- `docs/releases/` documents every release alongside verification notes.

## Rollback
- `shelldone rollback <version>` reverts the binary and (when possible) migrates configs back.
- Automatic restore points are created before upgrades (state plus configs).

## Release Quality Monitoring
- Post-release checklist tracks metrics (crash rate, performance), feedback (issues/discussions), and update adoption.
- Canary rollouts follow the nightly → beta → stable flow with SLO enforcement (see `docs/architecture/observability.md`).

## Next Steps
1. Publish an ADR for the release/signing pipeline.
2. Implement `shelldone ship` (extending the Makefile target) and wire it into CI.
3. Build the migration engine (`shelldone_migrations`).
4. Ship an update/rollback guide (`docs/recipes/update-guide.md`).
5. Provide plugin/theme catalogues with explicit compatibility metadata.

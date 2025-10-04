# Capability Marketplace Hooks

## Purpose
Provide a safe distribution channel for capability bundles (plugins, adapters, policies) with automatic Σ-cap negotiation and policy enforcement.

## Components
- **Marketplace Manifest (`marketplace/registry.json`):** lists bundles with metadata (id, version, capabilities, signatures).
- **Installer CLI (`shelldone marketplace install <bundle>`):** downloads, verifies signatures, updates Σ-cap profile.
- **Policy Check:** integrates with Rego (`config/policies/marketplace.rego`) to accept/deny capabilities.
- **Rollback:** one-command undo via snapshot + uninstall script.

## Workflow
1. Bundle signed (Ed25519) and published to registry.
2. User runs install → CLI fetches manifest, verifies signature, unpacks to `plugins/bundles/<id>/<version>`.
3. Installer posts `kind: "marketplace.install"` event to `shelldone-agentd /journal/event`.
4. Σ-cap profile updated (capabilities.yaml), policy guard approves or blocks.
5. On failure, installer rolls back and emits `kind: "marketplace.rollback"`.

## Security
- Signatures mandatory; revocation list stored in `marketplace/revocations.json`.
- Policy options: allow/deny/confirm with persona-based prompts.
- Telemetry: `marketplace.install.success`, `marketplace.install.failure`, aggregated in Prism.

## Implementation Plan
1. Define manifest schema (`docs/schema/marketplace-bundle.json`).
2. Implement CLI commands: `install`, `list`, `remove`, `verify`.
3. Integrate with Σ-cap handshake (capabilities advertised post-install).
4. Update verify pipeline: lint manifests, simulate install with `--dry-run`.
5. Documentation for bundle authors (`docs/recipes/plugins.md` update).

## Rollback Plan
- `shelldone marketplace revert --bundle <id>@<version>` removes files, restores previous capabilities, emits journal event.
- Snapshot saved in `state/marketplace/<timestamp>.json` before each install.

## ADR Reference
- ADR-0004 (Capability Marketplace Hooks).

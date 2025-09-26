# Security, Secrets, and Access Control

## Principles
1. **Least privilege:** every plugin, agent, or subsystem receives only the capabilities it needs.
2. **Transparency:** privileged actions are logged and auditable.
3. **Isolation:** untrusted components run in sandboxes (WASM, namespaces, containers).

## Threat Model
- Compromised plugin or extension.
- Leakage of keys and tokens (SSH, API credentials, agent certificates).
- Privilege escalation through syscalls, IPC, or unsafe command execution.
- Unauthorised agent access to user data.

## Secret Storage
- Primary mechanism: OS keyring (macOS Keychain, Windows Credential Vault, Secret Service).
- Additional layer `secrets/` (JSON + AES-GCM with a key sourced from the keyring) storing references and metadata.
- CLI commands: `shelldone secret add/list/revoke`, with expiration/rotation policies.

## Access Control
- `config/policies/*.yaml` define allowed actions (filesystem, network, shell commands).
- Agents rely on workflow approvals (manual + policy-based) with logs in `logs/agents.log`.
- RBAC: roles (owner/maintainer/contributor/viewer) mapped to capability sets (manage plugins, start agents, access UI areas).

## Plugins and Sandbox
- Rust plugins requiring elevated privileges must declare `trusted=true` and origin metadata.
- WASM plugins are sandboxed by default (no FS/network) and gain capabilities via host API grants.
- Resource limits: cgroups (Linux), job objects (Windows), `sandbox-exec` (macOS).

## Incident Response
- Signature verification for plugins/updates (ed25519).
- Revocation list distributed with updates.
- Response playbook lives in `docs/security/runbook.md`; include security-team contacts.

## Checks
- Static validation of policy/configs via `make verify` (extend `scripts/verify.py`).
- Secret scanning (git-secrets or custom check) in `make verify-full`.
- Containerised smoke tests in `make verify-full` (WASM sandbox + capability drops).

## Milestones
1. ADR for the secret/key-management model.
2. Implement secret manager API + CLI.
3. Enforce policy files at runtime.
4. Sandbox plugins with resource limits (aligned with perf budgets).
5. Finalise the incident runbook and revocation processes.

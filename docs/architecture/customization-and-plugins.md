# Customisation, Plugins, and IDE Features

## Goal
Deliver a modular platform where users and the community extend the terminal without forks—from themes and macros to full IDE flows.

## Plugin Model
- **Extension formats:**
  - Rust plugins (dynamic modules loaded at startup or on demand).
  - Lua scripts (hot reload, safe API surface).
  - WASM modules (isolated execution for third parties).
- **Capability negotiation:** каждая надстройка объявляет потребности через Σ-cap (`plugins/capabilities.yaml`) и может быть отключена политикой.
- **API layers:**
  1. Rendering (panels, tabs, overlays).
  2. Input/control (keys, gestures, command palette).
  3. Multiplexer (domains, sessions, pipes, remote connections).
  4. Services (file manager, LSP, debugger, automation).
- **Lifecycle:** registration → Σ-cap compatibility check → sandbox (resource limits, persona hints) → events (`on_init`, `on_tick`, `on_event`).
- **Security:**
  - Plugins declare required privileges (filesystem, network, external processes) через Σ-cap.
  - Untrusted modules use WASM plus capability-based API; ACK `agent.guard` управляет временными допусками.

## Themes and Visual Presets
- Theme configuration in `config/themes/` (YAML/JSON/TOML).
- Hot switching via `shelldone theme apply <name>`.
- Ship a standard preset set and allow publishing to the catalogue.

## IDE Capabilities
- **File manager:** navigation pane, preview, Git integration.
- **LSP hub:** shared bus for language servers, diagnostics in the status bar.
- **Debugger:** DAP bridge (breakpoints, watch expressions, stepped execution).
- **Automation:** Lua/Rust playbooks for repeatable operations, вызываемые через ACK `agent.plan`.

## Implementation Plan
1. Extract core API and stabilise the `shelldone_api` crate.
2. Build the plugin loader + registry (`plugins/registry/`).
3. Ship SDKs and templates (`cargo generate shelldone-plugin`).
4. Provide an extension manager in UI and CLI.
5. Publish showcase examples (themes, LSP, file manager) with documentation.
6. Integrate capability marketplace hooks (see `docs/architecture/plugin-marketplace.md`).

More context: `docs/ROADMAP/2025Q4.md`, section “Plugin Ecosystem”.

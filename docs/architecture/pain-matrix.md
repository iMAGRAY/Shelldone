# Shelldone Pain Matrix

Status legend:
- ✅ — реализовано и покрыто тестами/документацией.
- 🟡 — частично реализовано; есть задел, требуется завершение.
- ❌ — отсутствует; описан roadmap/решение.

*Общий владелец всех инициатив — imagray `<magraytlinov@gmail.com>`; перечисленные гильдии выступают поддержкой.*

| # | Pain Point | Current Status | Existing Signals | Plan / Owner |
|---|------------|----------------|------------------|--------------|
|1|Multiline paste warning|🟡|`canonicalize_pasted_newlines`, nightly confirm overlay|UX guild · `task-persona-onboarding` (links to `UX-12`) — guided overlay + Lua hook.|
|2|ANSI/escape sanitization|🟡|Σ-pty sandbox + `sigma.guard`|Security guild · `task-security-pipeline` (`SEC-04`) — ship default policy presets + admin docs.|
|3|LLM hidden commands guard|❌|—|ACK guild · backlog `ACK-07` → оформить `task-playbooks-v2` step “approval UI”.|
|4|Resilient SSH (mosh-class)|🟡|Mux daemon, detach/reattach|Add roaming transport (QUIC keepalive) & reconnect heuristics (`MUX-05`).|
|5|Advanced graphics|✅|Kitty/iTerm2/Sixel support|Maintain allowlist + perf tests (`GFX-01`).|
|6|Clipboard bridge|🟡|`ClipboardBridgeService` (wl-copy/xclip/clip.exe) + policy `termbridge_allow`, OSC52 write|Expose read whitelist UI + consent UX (`CLIP-02`), tmux passthrough glue.|
|7|PRIMARY vs CLIPBOARD clarity|🟡|Selectable sources via commands|Display channel indicator + persona hints (`UX-13`).|
|8|Unicode/emoji/ligatures|✅|HarfBuzz + char-props|Continue upstream sync (`INTL-01`).|
|9|History audit|❌|—|Governance guild · `task-history-continuum` (ties to roadmap `AUD-02`).|
|10|Poison log scrubbing|🟡|`strip-ansi-escapes` crate|Wire into journald/log sinks + sigma policy (`LOG-03`).|
|11|SIGHUP resilience|🟡|Mux preserves panes|Document guarantees + extend for remote shells (`MUX-06`).|
|12|Ctrl-S freeze handling|❌|—|Offer toggle to disable XON/XOFF per pane (`TTY-01`).|
|13|Project env activation|❌|—|Implement direnv/nvm hooks via agentd (`ENV-04`).|
|14|Command palette|✅|Command palette + Lua augment|Add agent actions (`UX-02`).|
|15|Fuzzy search output|🟡|QuickSelect/launcher|Unify fuzzy across scrollback + Continuum snapshots (`UX-07`).|
|16|Long-log performance|✅|GPU renderer + perf budgets|Maintain perf CI (`PERF-Σ`).|
|17|Onboarding/TL;DR|❌|—|Docs guild · `task-persona-onboarding` + `DOC-01` docs site.|
|18|Session record/replay|✅|`wezterm record` tooling|Expose Continuum replay UI (`REC-02`).|
|19|tmux + system clipboard|🟡|OSC52 bridging + system backends via ClipboardBridgeService|Ship tmux helper + policy doc (`CLIP-03`).|
|20|SSH key/ForwardAgent policy|❌|—|Extend `shelldone-ssh` with forward-agent guardrails (`SEC-05`).|
|21|TUI previews|✅|Kitty graphics|Add test matrix (`GFX-02`).|
|22|Themes/contrast|🟡|Palette catalog, `text_min_contrast_ratio` nightly|Promote auto-contrast to GA (`UX-15`).|
|23|Shell lint / quoting|❌|—|Bundle shellcheck/agent autop-run (`DEV-06`).|
|24|Activity telemetry|🟡|Continuum journal + `sigma.guard.events`|Deliver per-user dashboards + OTLP streams (`OBS-04`).|
|25|IDE layer (LSP/DAP)|🟡|`plugins/`, Lua APIs, new `AgentBindingService` domain|Stabilize plugin SDK + sample packs (`PLUGIN-01`), wire agent bindings to devtools catalog.|
|26|Terminal orchestration drift|🟡|TermBridgeService `/termbridge/*` routes, capability snapshot в `/status`, **new `/termbridge/cwd` sync**|TermBridge rollout · `task-termbridge-core`, `task-termbridge-discovery`, `task-termbridge-security`.|
|27|Bracketed paste UX|🟡|Partial overlay в Shelldone GUI|UX guild · `task-termbridge-paste-guard`, persona presets + hint budgets.|
|28|Clipboard parity (Wayland/tmux)|🟡|Wayland X11 Windows backends live; tmux passthrough pending|Platform guild · `task-termbridge-clipboard` (tmux passthrough + batching telemetry).|
|29|IPC race / window targeting|🟡|Ad-hoc pane id tracking|TermBridge guild · `task-termbridge-core` + `task-termbridge-test-suite` (tokenized bindings, focus checks).|
|30|Platform docs TL;DR|❌|—|Docs guild · `task-termbridge-discovery` (card stack для kitty/WT/WezTerm/Alacritty).|

See `docs/architecture/agent-governance.md` and `docs/architecture/utif-sigma.md` for component details.

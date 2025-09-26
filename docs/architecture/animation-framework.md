# Shelldone Animation and Effects Platform

## Vision
Enable wildly expressive animations that never slow the terminal—anything from text dissolving into particles to reactive effects triggered by user actions.

## Rendering Architecture
- **Backend:**
  - GPU path: `wgpu` (Metal/DX/Vulkan) with OpenGL fallback.
  - CPU path: optimised rasteriser for headless/low-end environments.
- **Effect pipeline:**
  1. Event → `EffectDescriptor` generation.
  2. Scheduler assigns effects to layers (background, text, UI, overlay).
  3. Shaders/pipelines receive batched geometry and attribute data.
  4. Result composited with the terminal scene.
- **Budgets:**
  - 16.6 ms per frame (60 FPS target).
  - Dedicated limits for VRAM/CPU (see `docs/architecture/perf-budget.md`).

## Effect Model
- **Categories:**
  - Glyph-level (state machines per character: smoke, sparks, droplets).
  - Pane-level (waves, parallax, layout pulse).
  - System-level (animated cursor, tab transitions, backgrounds).
- **Configuration:**
  - `config/effects/*.yaml` defines parameters (duration, easing, shader module).
  - Randomisation support (seed) and context hooks (command, file type, agent action).
- **API:**
  - Rust `Effect` trait with Lua/WASM adapters.
  - Events: `on_char_added`, `on_char_removed`, `on_tab_switch`, `on_agent_action`.
  - Effect chains and conditional rules supported.

## Performance Strategy
- **Caching:** glyph textures in an atlas, particle pools.
- **Adaptive quality:** degrade gracefully under load (reduce detail, disable heavy effects).
- **Profiling:** dev mode overlay showing frame time and active effects.

## Roadmap
1. Integrate `wgpu` and refactor the render loop (see roadmap milestone “Animation Engine”).
2. Ship baseline effects (smoke, flash, neon trail, interference).
3. Provide SDK for custom effects (Rust template + Lua DSL).
4. Build an effect editor (live preview + config generator).
5. Launch a community gallery (`shelldone effect publish`).

## Documentation
- Theming/effect guide: `docs/config/themes.md` (to be updated alongside implementation).
- Animation cookbook: `docs/recipes/animations.md` (populate as features land).

All changes require render tests (snapshot + perf) and ADR entries where applicable.

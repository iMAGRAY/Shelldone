# Shelldone Performance Budgets

This document is the single source of truth for performance targets. Any change affecting latency or resource consumption must align with these budgets.

## Key Metrics
- **Input-to-render latency:** ≤ 20 ms (target 12 ms) for text input; UTIF-Σ target p95 15 ms, p99 25 ms.
- **Tab switch:** ≤ 80 ms (target 50 ms) until ready for input.
- **Handshake (Σ-cap round-trip):** ≤ 5 ms.
- **ACK overhead:** ≤ 3 ms per command before shell runtime.
- **Continuum restore:** ≤ 150 ms to resume full workspace.
- **Startup time (CLI → ready):** ≤ 400 ms (target 250 ms) on modern laptops.
- **Memory footprint:** baseline session ≤ 150 MB; each active domain ≤ 60 MB.
- **GPU frame time:** ≤ 16.6 ms at 60 FPS (animations must adapt dynamically).

## Profiling
- Performance scripts reside in `scripts/perf/` (to be populated via the relevant epic) and run microbenchmarks plus end-to-end tests (k6 open-model 3×60 s, warmup 15 s).
- Results land in `artifacts/perf/` and are analysed in CI (`make verify VERIFY_MODE=full`).
- Regression analysis uses `cargo bench` + `criterion` (see `docs/recipes/perf.md`).
- ACK/Σ-cap scenarios recorded as JSON baseline (`artifacts/perf/utif-sigma/*.json`).
- Σ-pty proxy benchmark (TODO: `scripts/perf/utif_pty.js`) to ensure proxy overhead ≤3 ms.

## Fallback and Degradation
- If a budget is exceeded the render engine reduces quality (see `docs/architecture/animation-framework.md`).
- Heavy operations (e.g. large `git status`) expose async indicators and cancellation hooks.
- Nothing may block the event loop; long tasks must be asynchronous.

## Quality Control
- `make verify` (`VERIFY_MODE=full`) executes perf smoke tests.
- Every new feature must document how it consumes budget and how to test it.

Review budgets quarterly (see `docs/ROADMAP/notes/`).

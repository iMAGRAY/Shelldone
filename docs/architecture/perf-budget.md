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
- `python3 scripts/verify.py` (`VERIFY_MODE=full|ci`) executes the `perf-probes` gate which spins up `shelldone-agentd`, runs the k6 probes three times each, and fails the pipeline if budgets regress.
- A deterministic **loopback runner** (`perf_runner --runner loopback`) replaces ad-hoc “stub” wording in tests; it mirrors telemetry wiring without spawning k6. CI uses it for fast smoke validation before the heavy probes.
- Results land in `artifacts/perf/` and are analysed in CI (`VERIFY_MODE=full python3 scripts/verify.py`). Summaries replicate into `reports/perf/metrics.prom` for textfile scrape.
- Regression analysis uses `cargo bench` + `criterion` (see `docs/recipes/perf.md`).
- ACK/Σ-cap scenarios recorded as JSON baseline (`artifacts/perf/utif-sigma/*.json`).
- Σ-pty proxy benchmark (TODO: `scripts/perf/utif_pty.js`) to ensure proxy overhead ≤3 ms.

## Fallback and Degradation
- If a budget is exceeded the render engine reduces quality (see `docs/architecture/animation-framework.md`).
- Heavy operations (e.g. large `git status`) expose async indicators and cancellation hooks.
- Nothing may block the event loop; long tasks must be asynchronous.

## Quality Control
- `python3 -m perf_runner run` запускает те же пробы локально (поддерживает `--probe`, `--profile`, `--no-agentd`, env-переключатели). В fast-пайплайнах используйте `--runner loopback`, в release — `k6`.
- `python3 scripts/verify.py` (`VERIFY_MODE=full`) выполняет smoke плюс полные k6-пробы; `VERIFY_MODE=ci` оставляет loopback + ключевые k6 сценарии.
- Профили нагрузки (`dev`, `ci`, `full`, `staging`, `prod`) выбираются через `--profile` либо `SHELLDONE_PERF_PROFILE` и задают стандартные `SHELLDONE_PERF_*` значения.
- Every new feature must document how it consumes budget and how to test it.

### Status — 4 October 2025
- `python3 scripts/verify.py` (build #2025-10-04T22:48Z) зелёный; loopback smoke занимает ≈45 s, полные k6 прогоны — 3×65 s (утф-Σ, policy_perf).
- TermBridge discovery регистрирует терминалы через новый registry service; perf probe `termbridge_discovery` (`scripts/perf/termbridge_discovery.js`) в `python3 scripts/verify.py --mode full` следит за бюджетом (`p95 ≤200 ms`, `p99 ≤300 ms`) и фидит `termbridge.actions{command="discover"}` / latency dashboards.

Review budgets quarterly (see `docs/ROADMAP/notes/`).

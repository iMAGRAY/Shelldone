# Epic AI Automation — Increment Plan (5 Oct 2025)

## Goal
Advance the TermBridge + MCP stack to production-ready reliability by hardening contracts, live sync, and resilience paths.

## Scope (Sprint 2025-10-06 → 2025-10-17)
1. `task-mcp-bridge-contract-tests` — build gRPC contract suites covering handshake, streaming IO, and error propagation; gate on CI.
2. `task-termbridge-discovery-mcp-sync` — promote to in-progress, deliver delta watcher + pruning for registry without restart.
3. `task-termbridge-backpressure` — implement bounded queues/backpressure, ensure auto-recovery on overload.
4. `task-termbridge-core-telemetry` — close remaining 60% by wiring span IDs + Prism metrics for failure triage.

## Exit Criteria
- Contract tests fail on schema drift and run < 5 minutes in CI.
- Sync watcher prunes stale endpoints within 60 seconds; functional tests green.
- Overload scenario drains within 1 second of idle and recovers without manual intervention.
- Observability dashboards chart per-terminal latency/error rate with trace correlation.

## Dependencies & Risks
- Requires completion of `task-mcp-bridge` core handshake patches.
- Perf regression risk from added telemetry/backpressure — run perf mini-protocol before promoting.
- Coordination with QA for `task-termbridge-test-suite` once sync/backpressure land.

## Follow-up
- Schedule security review covering agent access policies once bridge contracts stabilise.
- Prep docs update for `docs/architecture/ai-integration.md` reflecting new flows.

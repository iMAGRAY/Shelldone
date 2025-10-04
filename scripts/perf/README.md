# Perf Scenarios (UTIF-Σ)

## k6 Setup
- Warmup: 15 s
- Runs: 3 × 60 s (constant arrival rate)
- Targets:
  - `p95` ≤ 15 ms
  - `p99` ≤ 25 ms
  - `error_rate` < 0.5%

## Scripts
- `utif_exec.js` — exercises Σ-cap handshake + ACK `agent.exec` loops.

## Usage
```bash
k6 run scripts/perf/utif_exec.js --vus 50 --duration 60s
```

Results are exported to `artifacts/perf/utif-sigma/*.json` by default.

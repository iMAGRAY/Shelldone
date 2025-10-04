# Performance Test Suite (UTIF-Σ)

## Overview
k6 performance baselines for shelldone-agentd control plane.

## Prerequisites
```bash
# Install k6
brew install k6  # macOS
# or
sudo gpg -k
sudo gpg --no-default-keyring --keyring /usr/share/keyrings/k6-archive-keyring.gpg --keyserver hkp://keyserver.ubuntu.com:80 --recv-keys C5AD17C747E3415A3642D57D77C6C491D6AC1D69
echo "deb [signed-by=/usr/share/keyrings/k6-archive-keyring.gpg] https://dl.k6.io/deb stable main" | sudo tee /etc/apt/sources.list.d/k6.list
sudo apt-get update
sudo apt-get install k6
```

## Performance Targets

| Metric | Target | Description |
|--------|--------|-------------|
| `p95` | ≤ 15 ms | 95th percentile latency |
| `p99` | ≤ 25 ms | 99th percentile latency |
| `error_rate` | < 0.5% | HTTP error rate |
| `policy_overhead` | < 5 ms | Policy evaluation overhead |

## Scripts

### `utif_exec.js`
**Purpose**: Baseline agent.exec performance under load

**Scenario**:
- 200 req/s for 60s
- Pre-allocated 50 VUs, max 100 VUs
- Executes `echo` command via shell

**Thresholds**:
- `utif_exec_latency`: p95≤15ms, p99≤25ms
- `utif_exec_errors`: <0.5%

**Usage**:
```bash
# Start agentd
cargo run -p shelldone-agentd -- --listen 127.0.0.1:17717

# Run test
k6 run scripts/perf/utif_exec.js
```

### `policy_perf.js`
**Purpose**: Policy enforcement overhead measurement

**Scenario**:
- 100 req/s for 30s
- 50% allowed requests (core persona)
- 50% denied requests (unknown persona)

**Thresholds**:
- `policy_allowed_latency`: p95≤20ms, p99≤30ms
- `policy_denied_latency`: p95≤10ms, p99≤15ms
- `policy_errors`: <1%

**Usage**:
```bash
# Run with default policy
k6 run scripts/perf/policy_perf.js
```

## CI Integration

### Makefile Targets
```bash
make perf-baseline    # Run all perf tests
make perf-exec        # Run exec baseline
make perf-policy      # Run policy overhead test
```

### GitHub Actions
```yaml
- name: Run performance regression check
  run: |
    cargo run -p shelldone-agentd &
    sleep 2
    k6 run --quiet scripts/perf/utif_exec.js
```

## Results Export

Results exported to `artifacts/perf/`:
```
artifacts/perf/
├── utif-sigma/
│   ├── utif_exec_<timestamp>.json
│   └── policy_perf_<timestamp>.json
└── summary.md
```

## Analyzing Results

### k6 JSON Output
```bash
k6 run --out json=results.json scripts/perf/utif_exec.js
jq '.metrics | {exec_p95: .utif_exec_latency.values["p(95)"], exec_p99: .utif_exec_latency.values["p(99)"]}' results.json
```

### Grafana Dashboard
Import `docs/dashboards/k6-perf.json` into Grafana for live monitoring.

## Troubleshooting

### High Latency
- Check CPU usage: `top -p $(pgrep shelldone-agentd)`
- Profile with: `cargo flamegraph -p shelldone-agentd`
- Increase file descriptors: `ulimit -n 10000`

### Policy Evaluation Slow
- Enable hot-reload caching in policy engine
- Reduce Rego policy complexity
- Profile with `--log-level=debug`

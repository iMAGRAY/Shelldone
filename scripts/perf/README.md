# Performance Test Suite (UTIF-Σ)

## Overview
k6-based performance probes covering the Shelldone agent control plane. The suite powers the `perf-probes` gate in `make verify` (modes `full` and `ci`) and exports reproducible artifacts under `artifacts/perf/`.

## Prerequisites
```bash
# Install k6
brew install k6  # macOS
# or
sudo gpg -k
sudo gpg --no-default-keyring --keyring /usr/share/keyrings/k6-archive-keyring.gpg --keyserver hkp://keyserver.ubuntu.com:80 --recv-keys C5AD17C747E3415A
echo "deb [signed-by=/usr/share/keyrings/k6-archive-keyring.gpg] https://dl.k6.io/deb stable main" | sudo tee /etc/apt/sources.list.d/k6.list
sudo apt-get update
sudo apt-get install k6
```

## Verify Integration
`make verify-full`/`make verify-ci` start `shelldone-agentd`, run three trials for each probe, enforce budgets, and emit:

```
artifacts/perf/
├── agentd_perf.log
├── summary.json
├── policy_perf/
│   ├── policy_perf_trial1.json
│   ├── policy_perf_trial2.json
│   ├── policy_perf_trial3.json
│   └── summary.json
└── utif_exec/
    ├── utif_exec_trial1.json
    ├── utif_exec_trial2.json
    ├── utif_exec_trial3.json
    └── summary.json
```

`summary.json` captures pass/fail status for every budget; individual probe directories contain the per-trial k6 exports plus an aggregated summary used by dashboards.

## Performance Targets

| Metric | Target | Description |
|--------|--------|-------------|
| `latency_p95_ms` | ≤ 15 ms | 95th percentile end-to-end latency for `agent.exec` |
| `latency_p99_ms` | ≤ 25 ms | 99th percentile end-to-end latency |
| `error_rate_ratio` | < 0.005 | Error ratio (0.5 %) under steady load |
| `policy_allowed_latency` | ≤ 20 ms | Policy pass path latency |
| `policy_denied_latency` | ≤ 10 ms | Policy deny path latency |
| `policy_error_rate_ratio` | < 0.01 | Error ratio (1 %) for policy scenario |

Budgets are enforced after the median across three trials; failing probes unblock only once metrics drop below targets again.

## Scripts

### `utif_exec.js`
Measures `agent.exec` latency under sustained load.

**Scenario defaults**
- Rate: 200 req/s (`SHELLDONE_PERF_RATE`)
- Duration: 60s (`SHELLDONE_PERF_DURATION`)
- Warm-up offset: 0s (`SHELLDONE_PERF_WARMUP_SEC`)
- Pre-allocated VUs: 50 (`SHELLDONE_PERF_VUS`)
- Max VUs: 100 (`SHELLDONE_PERF_MAX_VUS`)

### `policy_perf.js`
Blends allowed/denied policy decisions (50/50 split) to track governance overhead.

**Scenario defaults**
- Rate: 100 req/s (`SHELLDONE_PERF_POLICY_RATE` or `SHELLDONE_PERF_RATE`)
- Duration: 30s (`SHELLDONE_PERF_POLICY_DURATION` or `SHELLDONE_PERF_DURATION`)
- Warm-up offset inherits `SHELLDONE_PERF_POLICY_WARMUP_SEC` (fallback to `SHELLDONE_PERF_WARMUP_SEC`).

Both scripts honour the environment variables listed below when executed via `k6 run` or through the verify pipeline.

## Environment Overrides

| Variable | Default | Applies to | Notes |
|----------|---------|------------|-------|
| `SHELLDONE_PERF_RATE` | 200 | utif_exec | Requests per second. |
| `SHELLDONE_PERF_DURATION` | 60s | utif_exec & fallback | ISO-like duration string accepted by k6. |
| `SHELLDONE_PERF_VUS` | 50 | utif_exec | Pre-allocated virtual users. |
| `SHELLDONE_PERF_MAX_VUS` | 100 | utif_exec | Maximum virtual users. |
| `SHELLDONE_PERF_WARMUP_SEC` | 0 | both | Delay before scenario start (seconds). |
| `SHELLDONE_PERF_TRIALS` | 3 | verify.py | Number of trials per probe. |
| `SHELLDONE_PERF_POLICY_RATE` | 100 | policy_perf | Overrides mix scenario rate. |
| `SHELLDONE_PERF_POLICY_DURATION` | 30s | policy_perf | Overrides duration for policy mix. |
| `SHELLDONE_PERF_POLICY_VUS` | 30 | policy_perf | Overrides VUs for policy mix. |
| `SHELLDONE_PERF_POLICY_MAX_VUS` | 60 | policy_perf | Overrides max VUs for policy mix. |
| `SHELLDONE_PERF_POLICY_WARMUP_SEC` | inherit | policy_perf | Warm-up for policy probe only. |

## Local Usage
```bash
# One-shot run (agentd + обе пробы)
python3 -m perf_runner run

# Только utif_exec без запуска agentd
python3 -m perf_runner run --probe utif_exec --no-agentd

# Настройка длительности и числа прогонов
SHELLDONE_PERF_TRIALS=1 SHELLDONE_PERF_DURATION=20s python3 -m perf_runner run
```

CLI автоматически стартует `shelldone-agentd` (если не передан `--no-agentd`), пишет лог в `agentd_perf.log` и завершает работу с ненулевым кодом при нарушении бюджетов.

## Profiles

```bash
python3 -m perf_runner run --profile dev      # trials=1, 20s длительность
python3 -m perf_runner run --profile ci       # trials=1, 30s длительность
python3 -m perf_runner run --profile full     # trials=3, 60s длительность
python3 -m perf_runner run --profile staging  # trials=2, 45s длительность
python3 -m perf_runner run --profile prod     # trials=5, 120s длительность
```

Профиль можно переопределять отдельными флагами (`--trials`, `--warmup-sec`, env `SHELLDONE_PERF_*`).

Подробнее о textfile ingest: `docs/observability/prometheus-textfile.md`.

### Production & Staging
- `--profile staging` — двухпрогонный smoke (45 s, rate 200/110) перед выкладкой.
- `--profile prod` — полный контракт (5 прогонов по 120 s, rate 240/150); используем в ночных jobах и при релизе.
- Для кастомизации CI можно комбинировать `SHELLDONE_PERF_PROFILE=ci` с точечными `SHELLDONE_PERF_TRIALS=2` и т.д.

## CI / Make Targets
```bash
make perf-utif         # python3 -m perf_runner run --probe utif_exec
make perf-policy       # python3 -m perf_runner run --probe policy_perf
make perf-baseline     # python3 -m perf_runner run (оба прогона)
make verify-full       # включает perf-probes gate + артефакты
```

## Analyzing Results
```bash
jq '.aggregated.latency.value' artifacts/perf/utif_exec/summary.json
jq '.probes' artifacts/perf/summary.json
```

## Troubleshooting
- **Missing k6**: install the binary (see prerequisites).
- **High latency**: profile `shelldone-agentd` with `cargo flamegraph -p shelldone-agentd` and inspect `agentd_perf.log`.
- **Policy budgets failing**: inspect `artifacts/perf/policy_perf/*.json` for outliers and review Rego rules.

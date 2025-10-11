# Prometheus Textfile Ingest for Perf Metrics

Shelldone produces OpenMetrics under `reports/perf/metrics.prom` after every perf run. This document explains how to ingest the file via the node-exporter textfile collector.

## Workflow

1. **Run perf suite**
   ```bash
   python3 -m perf_runner run --profile ci
   ```
2. **Install the textfile for node-exporter**
   ```bash
   install -Dm644 reports/perf/metrics.prom      /var/lib/node_exporter/textfile_collector/shelldone_perf.prom
   ```
3. **Ensure node-exporter is configured** with `--collector.textfile.directory=/var/lib/node_exporter/textfile_collector`.
4. **Scrape configuration**:
   ```yaml
   scrape_configs:
     - job_name: shelldone-textfile
       static_configs:
         - targets: ['node-exporter:9100']
   ```

Exported metric format:
```
shelldone_perf_metric{probe="utif_exec",metric="latency_p95_ms",unit="ms"} 12.4
```

## GitHub Actions example
```yaml
- name: Run perf suite
  run: |
    python3 -m perf_runner run --profile ci       --summary-path reports/perf/ci_summary.json       --prom-path reports/perf/ci_metrics.prom
- name: Publish textfile artifact
  uses: actions/upload-artifact@v4
  with:
    name: shelldone-perf-metrics
    path: reports/perf/ci_metrics.prom
```

## Verification
- `SHELLDONE_PERF_PROFILE=ci python3 -m perf_runner run` generates the file automatically.
- `scripts/status.py --json` exposes `perf.last_verify.metrics_prom` so CD pipelines can sync the latest file.
- `scripts/tests/test_perf_runner.py::test_prometheus_renderer` ensures the exporter emits data.

## GitHub Actions (full pipeline)
```yaml
name: Perf Gate
on: [push]
jobs:
  perf:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-python@v5
        with:
          python-version: '3.11'
      - name: Install deps
        run: pip install -r requirements-ci.txt
      - name: Perf suite
        run: python3 -m perf_runner run --profile ci --summary-path reports/perf/ci_summary.json --prom-path reports/perf/ci_metrics.prom
      - name: Upload perf summary
        uses: actions/upload-artifact@v4
        with:
          name: perf-summary
          path: |
            reports/perf/ci_summary.json
            reports/perf/ci_metrics.prom
```

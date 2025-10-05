from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:  # pragma: no cover
    from .app.service import SuiteReport


def render_prometheus_metrics(suite_report: "SuiteReport") -> str:
    lines: list[str] = ["# TYPE shelldone_perf_metric gauge"]
    for report in suite_report.reports:
        for alias, metric in report.aggregated.items():
            value = metric.value
            lines.append(
                f'shelldone_perf_metric{{probe="{report.probe_id}",metric="{alias}",unit="{metric.unit}"}} {value}'
            )
    return "\n".join(lines) + "\n"

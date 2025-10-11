#!/usr/bin/env python3
"""Validate OTLP metrics payload against TermBridge expectations."""

from __future__ import annotations

import argparse
import base64
import json
import pathlib

from otlp_metrics_parser import DataPoint, Metric, parse_export_metrics


def load_metrics(path: pathlib.Path) -> list[Metric]:
    raw = json.loads(path.read_text(encoding="utf-8"))
    metrics: list[Metric] = []
    for entry in raw:
        if not str(entry.get("path", "")).endswith("/v1/metrics"):
            continue
        payload_b64 = entry.get("body_b64") or ""
        if not payload_b64:
            continue
        payload = base64.b64decode(payload_b64)
        metrics.extend(parse_export_metrics(payload))
    return metrics


def load_snapshot(path: pathlib.Path) -> list[str]:
    snapshot = json.loads(path.read_text(encoding="utf-8"))
    return [entry["terminal"] for entry in snapshot.get("terminals", [])]


def collect_terminals(metric: Metric) -> dict[str, list[DataPoint]]:
    mapping: dict[str, list[DataPoint]] = {}
    for datapoint in metric.datapoints:
        terminal = datapoint.attributes.get("terminal")
        if terminal:
            mapping.setdefault(terminal, []).append(datapoint)
    return mapping


def find_metric(metrics: list[Metric], name: str) -> Metric | None:
    for metric in metrics:
        if metric.name == name:
            return metric
    return None


def main() -> int:
    parser = argparse.ArgumentParser(description="Check OTLP metrics for TermBridge")
    parser.add_argument("--payload", required=True, type=pathlib.Path)
    parser.add_argument("--snapshot", required=True, type=pathlib.Path)
    args = parser.parse_args()

    metrics = load_metrics(args.payload)
    if not metrics:
        raise SystemExit("No OTLP metrics found in payload")

    target_metric = find_metric(metrics, "termbridge.capability.update")
    if target_metric is None:
        available = ", ".join(sorted(metric.name for metric in metrics)) or "<none>"
        raise SystemExit(f"termbridge.capability.update missing (found: {available})")

    expected_terminals = set(load_snapshot(args.snapshot))
    if not expected_terminals:
        raise SystemExit("Snapshot does not contain terminals")

    terminal_map = collect_terminals(target_metric)
    missing = expected_terminals - set(terminal_map.keys())
    if missing:
        raise SystemExit(
            "termbridge.capability.update missing terminals: " + ", ".join(sorted(missing))
        )

    allowed_changes = {"added", "updated"}
    for terminal, datapoints in terminal_map.items():
        if not any(datapoint.attributes.get("change") in allowed_changes for datapoint in datapoints):
            raise SystemExit(
                f"termbridge.capability.update lacks change attribute in {allowed_changes} for terminal {terminal}"
            )
        if not any(datapoint.attributes.get("source") for datapoint in datapoints):
            raise SystemExit(f"termbridge.capability.update missing source attribute for terminal {terminal}")
        if not any(datapoint.value >= 1.0 for datapoint in datapoints):
            raise SystemExit(f"termbridge.capability.update reports zero value for terminal {terminal}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())

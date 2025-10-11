#!/usr/bin/env python3
"""TermBridge compatibility matrix validation."""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import platform
import subprocess
import sys
import tempfile
from typing import Dict, List

BUDGET_MS = 200.0


def repo_root() -> pathlib.Path:
    return pathlib.Path(__file__).resolve().parents[2]


DASHBOARD_BASELINE = (
    repo_root()
    / "dashboards"
    / "baselines"
    / "termbridge"
    / "monitored_capabilities.json"
)
DASHBOARD_EXPORT_DIR = (
    repo_root() / "dashboards" / "artefacts" / "termbridge"
)

DEFAULT_MONITORED = {
    "darwin": {
        "wezterm": [
            "spawn",
            "split",
            "focus",
            "send_text",
            "clipboard_write",
            "clipboard_read",
        ]
    },
    "windows": {
        "wezterm": [
            "spawn",
            "split",
            "focus",
            "send_text",
            "clipboard_write",
            "clipboard_read",
        ]
    },
}


def default_output_path() -> pathlib.Path:
    return repo_root() / "artifacts" / "termbridge" / "capability-map.json"


def supports_termbridge_export(binary: pathlib.Path) -> bool:
    try:
        result = subprocess.run(
            [str(binary), "--help"],
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
        )
    except OSError:
        return False
    return "--termbridge-export" in (result.stdout or "")


def resolve_base_command(binary: pathlib.Path | None) -> List[str]:
    if binary is not None:
        if not binary.exists():
            raise SystemExit(f"--binary path does not exist: {binary}")
        if binary.is_dir():
            raise SystemExit(f"--binary must reference an executable file: {binary}")
        if not supports_termbridge_export(binary):
            raise SystemExit(f"--binary does not support --termbridge-export: {binary}")
        return [str(binary)]
    root = repo_root()
    candidates = [
        root / "target" / "debug" / "shelldone-agentd",
        root / "target" / "release" / "shelldone-agentd",
    ]
    for candidate in candidates:
        if candidate.exists() and supports_termbridge_export(candidate):
            return [str(candidate)]
    return ["cargo", "run", "--quiet", "-p", "shelldone-agentd", "--"]


def build_command(
    args: argparse.Namespace, state_dir: pathlib.Path, output_path: pathlib.Path
) -> List[str]:
    command = resolve_base_command(getattr(args, "binary", None))
    extra: List[str] = [
        "--state-dir",
        str(state_dir),
        "--termbridge-export",
        str(output_path),
        "--termbridge-export-timeout-ms",
        str(args.timeout_ms),
    ]
    if args.emit_otlp:
        extra.append("--termbridge-export-emit-otlp")
    if args.otlp_endpoint:
        extra.extend(["--otlp-endpoint", args.otlp_endpoint])
    if command and command[-1] == "--":
        return command + extra
    return command + extra


def run_export(args: argparse.Namespace, output_path: pathlib.Path) -> None:
    output_path.parent.mkdir(parents=True, exist_ok=True)
    if output_path.exists():
        output_path.unlink()
    env = os.environ.copy()
    env.setdefault("RUST_LOG", "warn")
    root = repo_root()
    with tempfile.TemporaryDirectory(prefix="termbridge-state-") as temp_dir:
        command = build_command(args, pathlib.Path(temp_dir), output_path)
        try:
            subprocess.run(command, cwd=root, env=env, check=True)
        except subprocess.CalledProcessError as exc:
            raise SystemExit(
                f"TermBridge export failed with exit code {exc.returncode}: {' '.join(command)}"
            ) from exc


def expect(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def validate_capabilities(capabilities: dict) -> None:
    required = {
        "spawn",
        "split",
        "focus",
        "duplicate",
        "close",
        "send_text",
        "clipboard_write",
        "clipboard_read",
        "cwd_sync",
        "bracketed_paste",
        "max_clipboard_kb",
    }
    expect(set(capabilities.keys()) == required, "capability schema drift detected")
    for key, value in capabilities.items():
        if key == "max_clipboard_kb":
            expect(value is None or isinstance(value, int), "max_clipboard_kb must be int or null")
        else:
            expect(isinstance(value, bool), f"{key} capability must be boolean")


def validate_terminal(entry: dict) -> None:
    expect("terminal" in entry and isinstance(entry["terminal"], str), "terminal slug missing")
    expect(
        "display_name" in entry and isinstance(entry["display_name"], str),
        "display_name missing",
    )
    expect(
        isinstance(entry.get("requires_opt_in"), bool),
        "requires_opt_in must be boolean",
    )
    expect(
        isinstance(entry.get("source"), str) and entry["source"].strip() != "",
        "source must be a non-empty string",
    )
    expect(isinstance(entry.get("notes", []), list), "notes must be a list")
    capabilities = entry.get("capabilities")
    expect(isinstance(capabilities, dict), "capabilities must be an object")
    validate_capabilities(capabilities)


def validate_clipboard(entry: dict) -> None:
    expect(isinstance(entry.get("id"), str), "clipboard backend id must be string")
    channels = entry.get("channels")
    expect(
        isinstance(channels, list) and all(isinstance(channel, str) for channel in channels),
        "clipboard channels must be list of strings",
    )
    expect(isinstance(entry.get("can_read"), bool), "clipboard can_read must be boolean")
    expect(isinstance(entry.get("can_write"), bool), "clipboard can_write must be boolean")
    expect(isinstance(entry.get("notes", []), list), "clipboard notes must be a list")


def validate_snapshot(snapshot: dict) -> None:
    expect(snapshot.get("version") == 1, "snapshot version mismatch")
    expect(isinstance(snapshot.get("generated_at"), str), "generated_at must be a string")
    discovery_ms = float(snapshot.get("discovery_ms", -1.0))
    expect(discovery_ms >= 0, "discovery_ms missing")
    expect(
        discovery_ms <= BUDGET_MS,
        f"TermBridge discovery latency {discovery_ms:.1f} ms exceeds {BUDGET_MS:.1f} ms budget",
    )
    diff = snapshot.get("diff", {})
    for key in ("added", "updated", "removed"):
        expect(isinstance(diff.get(key, []), list), f"diff.{key} must be a list")

    totals = snapshot.get("totals", {})
    expect(isinstance(totals, dict), "totals must be present")

    terminals = snapshot.get("terminals", [])
    expect(isinstance(terminals, list), "terminals must be a list")
    for entry in terminals:
        validate_terminal(entry)
    expect(
        len({entry["terminal"] for entry in terminals}) == len(terminals),
        "duplicate terminal identifiers detected",
    )

    clipboard = snapshot.get("clipboard_backends", [])
    expect(isinstance(clipboard, list), "clipboard_backends must be a list")
    for entry in clipboard:
        validate_clipboard(entry)

    expect(
        totals.get("terminals") == len(terminals),
        "totals.terminals mismatch",
    )
    expect(
        totals.get("clipboard_backends") == len(clipboard),
        "totals.clipboard_backends mismatch",
    )


def expect_terminal(snapshot: dict, terminal: str, capability: str) -> None:
    for entry in snapshot.get("terminals", []):
        if entry.get("terminal") == terminal:
            notes = [note.lower() for note in entry.get("notes", [])]
            missing = [
                note
                for note in notes
                if "not found" in note or "missing" in note
            ]
            expect(
                not missing,
                f"{terminal} terminal notes indicate missing binary: {entry.get('notes', [])}",
            )
            capabilities = entry.get("capabilities", {})
            expect(
                capabilities.get(capability),
                f"{terminal} capability '{capability}' is disabled; ensure CLI is installed",
            )
            return
    expect(False, f"{terminal} terminal missing from capability snapshot")


def enforce_ci_expectations(snapshot: dict) -> None:
    if not os.environ.get("CI"):
        return
    system = platform.system().lower()
    if system.startswith("win"):
        expect_terminal(snapshot, "wezterm", "spawn")
    elif system == "darwin":
        expect_terminal(snapshot, "wezterm", "spawn")


def normalized_os() -> str:
    system = platform.system().lower()
    if system.startswith("win"):
        return "windows"
    if system == "darwin":
        return "darwin"
    return "linux"


def load_monitored_baseline() -> Dict[str, Dict[str, Dict[str, bool]]]:
    if DASHBOARD_BASELINE.exists():
        return json.loads(DASHBOARD_BASELINE.read_text(encoding="utf-8"))
    return {}


def save_monitored_baseline(baseline: Dict[str, Dict[str, Dict[str, bool]]]) -> None:
    DASHBOARD_BASELINE.parent.mkdir(parents=True, exist_ok=True)
    DASHBOARD_BASELINE.write_text(
        json.dumps(baseline, indent=2, sort_keys=True), encoding="utf-8"
    )


def build_default_baseline(
    actual_map: Dict[str, Dict[str, bool]], os_key: str
) -> Dict[str, Dict[str, bool]]:
    defaults = DEFAULT_MONITORED.get(os_key, {})
    baseline: Dict[str, Dict[str, bool]] = {}
    for terminal, caps in defaults.items():
        actual_caps = actual_map.get(terminal, {})
        baseline[terminal] = {cap: actual_caps.get(cap) for cap in caps}
    return baseline


def write_dashboard_snapshot(snapshot: dict, os_key: str) -> pathlib.Path:
    DASHBOARD_EXPORT_DIR.mkdir(parents=True, exist_ok=True)
    export = {
        "os": os_key,
        "generated_at": snapshot.get("generated_at"),
        "discovery_ms": snapshot.get("discovery_ms"),
        "totals": snapshot.get("totals"),
        "terminals": snapshot.get("terminals"),
        "clipboard_backends": snapshot.get("clipboard_backends"),
    }
    path = DASHBOARD_EXPORT_DIR / f"{os_key}.json"
    path.write_text(json.dumps(export, indent=2, sort_keys=True), encoding="utf-8")
    return path


def write_drift_report(
    snapshot: dict,
    os_key: str,
    mismatches: List[Dict[str, object]],
    expected: Dict[str, Dict[str, bool]],
    actual: Dict[str, Dict[str, bool]],
) -> pathlib.Path:
    DASHBOARD_EXPORT_DIR.mkdir(parents=True, exist_ok=True)
    payload = {
        "os": os_key,
        "generated_at": snapshot.get("generated_at"),
        "mismatches": mismatches,
        "expected": expected,
        "actual": actual,
    }
    path = DASHBOARD_EXPORT_DIR / f"{os_key}-drift.json"
    path.write_text(json.dumps(payload, indent=2, sort_keys=True), encoding="utf-8")
    return path


def check_capability_drift(
    snapshot: dict, allow_drift: bool, update_baseline: bool
) -> tuple[List[Dict[str, object]], Dict[str, Dict[str, bool]], Dict[str, Dict[str, bool]], str]:
    baseline = load_monitored_baseline()
    os_key = normalized_os()
    actual_map = {
        entry.get("terminal"): entry.get("capabilities", {})
        for entry in snapshot.get("terminals", [])
    }
    expected = baseline.get(os_key)
    monitored_actual: Dict[str, Dict[str, bool]] = {}
    mismatches: List[Dict[str, object]] = []

    if expected is None:
        if update_baseline:
            new_entry = build_default_baseline(actual_map, os_key)
            if new_entry:
                baseline[os_key] = new_entry
                save_monitored_baseline(baseline)
                expected = new_entry
        return mismatches, monitored_actual, expected or {}, os_key

    for terminal, expected_caps in expected.items():
        actual_caps = actual_map.get(terminal)
        if actual_caps is None:
            mismatches.append(
                {
                    "terminal": terminal,
                    "capability": None,
                    "expected": expected_caps,
                    "actual": None,
                }
            )
            monitored_actual[terminal] = {}
            continue
        subset = {
            cap: actual_caps.get(cap)
            for cap in expected_caps.keys()
        }
        monitored_actual[terminal] = subset
        for cap_name, expected_value in expected_caps.items():
            actual_value = subset.get(cap_name)
            if actual_value != expected_value:
                mismatches.append(
                    {
                        "terminal": terminal,
                        "capability": cap_name,
                        "expected": expected_value,
                        "actual": actual_value,
                    }
                )

    if update_baseline and monitored_actual:
        baseline[os_key] = monitored_actual
        save_monitored_baseline(baseline)
        mismatches = []
        expected = baseline.get(os_key, {})

    return mismatches, monitored_actual, expected, os_key


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Validate TermBridge capability snapshot")
    parser.add_argument(
        "--output",
        type=pathlib.Path,
        default=None,
        help="Snapshot output path (default: artifacts/termbridge/capability-map.json)",
    )
    parser.add_argument(
        "--binary",
        type=pathlib.Path,
        help="Path to an existing shelldone-agentd binary",
    )
    parser.add_argument(
        "--timeout-ms",
        type=int,
        default=2000,
        help="Discovery timeout in milliseconds (default: 2000)",
    )
    parser.add_argument(
        "--emit-otlp",
        action="store_true",
        help="Emit OTLP telemetry during export",
    )
    parser.add_argument(
        "--otlp-endpoint",
        help="Override OTLP endpoint when emitting telemetry",
    )
    parser.add_argument(
        "--allow-drift",
        action="store_true",
        help="Allow monitored capability drift without failing",
    )
    parser.add_argument(
        "--update-baseline",
        action="store_true",
        help="Update monitored capability baseline for the current platform",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    output_path = args.output or default_output_path()
    run_export(args, output_path)
    if not output_path.exists():
        raise SystemExit(f"export did not produce snapshot at {output_path}")
    snapshot = json.loads(output_path.read_text(encoding="utf-8"))
    validate_snapshot(snapshot)
    enforce_ci_expectations(snapshot)
    allow_drift = args.allow_drift or not os.environ.get("CI")
    mismatches, monitored_actual, expected_caps, os_key = check_capability_drift(
        snapshot, allow_drift, args.update_baseline
    )
    dashboard_path = write_dashboard_snapshot(snapshot, os_key)
    if mismatches:
        drift_path = write_drift_report(
            snapshot, os_key, mismatches, expected_caps, monitored_actual
        )
        if not allow_drift:
            raise SystemExit(
                "Capability drift detected; see drift report at "
                f"{drift_path}"
            )
        else:
            print(f"Capability drift tolerated (allow_drift); report: {drift_path}")
    elif args.update_baseline:
        print(f"Updated baseline for {os_key} at {DASHBOARD_BASELINE}")
    totals = snapshot["totals"]
    discovery_ms = snapshot["discovery_ms"]
    print(
        f"TermBridge matrix OK — terminals: {totals['terminals']}, "
        f"clipboard backends: {totals['clipboard_backends']}, "
        f"discovery: {discovery_ms:.1f} ms; dashboard={dashboard_path}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

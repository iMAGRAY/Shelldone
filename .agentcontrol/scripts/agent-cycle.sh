#!/usr/bin/env bash
set -Eeuo pipefail
IFS=$'\n\t'
LC_ALL=C.UTF-8

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
REPORT_DIR="$ROOT/reports/agent_runs"
TIMESTAMP="$(date -u +%Y%m%dT%H%M%SZ)"
REPORT_PATH="$REPORT_DIR/$TIMESTAMP.yaml"

mkdir -p "$REPORT_DIR"

python3 "$SCRIPT_DIR/lib/architecture_tool.py" sync
python3 "$SCRIPT_DIR/lib/architecture_tool.py" check
"$SCRIPT_DIR/verify.sh" >/tmp/agent-cycle-verify.log

REPORT_PATH_ENV="$REPORT_PATH" SDK_ROOT_ENV="$ROOT" python3 - <<'PY'
import datetime as dt
import json
from pathlib import Path
import yaml

from os import environ

root = Path(environ["SDK_ROOT_ENV"])
manifest_path = root / "architecture" / "manifest.yaml"
report_path = Path(environ["REPORT_PATH_ENV"])
verify_log = Path("/tmp/agent-cycle-verify.log")
import sys

sys.path.insert(0, str(root))
from scripts.lib.architecture_tool import enrich_manifest

manifest = yaml.safe_load(manifest_path.read_text(encoding="utf-8"))
manifest = enrich_manifest(manifest)
program = manifest["program"]
summary = {
    "generated_at": dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
    "manifest_version": manifest["version"],
    "manifest_updated_at": manifest["updated_at"],
    "program_progress_pct": program["progress"]["progress_pct"],
    "epics": {epic["id"]: epic["status"] for epic in manifest.get("epics", [])},
    "big_tasks": {big["id"]: big["status"] for big in manifest.get("big_tasks", [])},
    "tasks": {task["id"]: task["status"] for task in manifest.get("tasks", [])},
    "verify_log_tail": verify_log.read_text(encoding="utf-8").splitlines()[-20:],
}
report_path.write_text(yaml.safe_dump(summary, sort_keys=False, allow_unicode=True), encoding="utf-8")
PY

echo "Создан отчёт: $REPORT_PATH"

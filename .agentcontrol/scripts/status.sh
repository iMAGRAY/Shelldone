#!/usr/bin/env bash
set -Eeuo pipefail
IFS=$'\n\t'
LC_ALL=C.UTF-8

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "$SCRIPT_DIR/lib/common.sh"
export SDK_ROOT

sdk::log "INF" "Синхронизация прогресса"
"$SDK_ROOT/scripts/progress.py" || sdk::log "WRN" "progress завершился с предупреждением"
printf '\n'
sdk::log "INF" "Синхронизация roadmap"
"$SDK_ROOT/scripts/sync-roadmap.sh" >/dev/null || sdk::log "WRN" "sync-roadmap завершился с предупреждением"

sdk::log "INF" "Roadmap summary"
ROADMAP_SKIP_PROGRESS=1 "$SDK_ROOT/scripts/roadmap-status.sh" compact
printf '\n'

sdk::log "INF" "Task board summary"
"$SDK_ROOT/scripts/task.sh" summary

PROJECT_ROOT="$(cd "$SDK_ROOT/.." && pwd)"
SOURCE_STATUS="$SDK_ROOT/reports/status.json"
TARGET_STATUS="$PROJECT_ROOT/reports/status.json"
mkdir -p "$PROJECT_ROOT/reports"
export SOURCE_STATUS TARGET_STATUS
python3 <<'PY'
import json
import os
from pathlib import Path

source = Path(os.environ["SOURCE_STATUS"])
target = Path(os.environ["TARGET_STATUS"])
if source.exists():
    data = json.loads(source.read_text(encoding="utf-8"))
    target.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
PY
unset SOURCE_STATUS TARGET_STATUS

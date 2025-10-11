#!/usr/bin/env bash
set -Eeuo pipefail
IFS=$'\n\t'
LC_ALL=C.UTF-8

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "$SCRIPT_DIR/lib/common.sh"

sdk::load_commands
REPORT_DIR="$SDK_ROOT/reports"
mkdir -p "$REPORT_DIR"
REPORT_FILE="$REPORT_DIR/doctor.json"

sdk::log "INF" "Анализ окружения"
python3 -m scripts.lib.deps_checker "$SDK_ROOT" >"$REPORT_FILE"

python3 - "$REPORT_FILE" <<'PY'
import json
import sys
from pathlib import Path
report = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
rows = report["results"]
problems = [r for r in rows if r["status"] == "missing"]
print(f"Итог: {len(rows)} проверок, {len(problems)} проблем.")
for row in rows:
    name = row["name"]
    status = row["status"]
    details = row.get("details") or ""
    fix = row.get("fix") or ""
    prefix = "✔" if status == "ok" or status == "detected" else "✖"
    line = f"{prefix} {name} — {status}"
    if details:
        line += f" ({details})"
    if fix and status == "missing":
        line += f" :: fix: {fix}"
    print(line)
sys.exit(1 if problems else 0)
PY

sdk::log "INF" "Отчёт сохранён: $REPORT_FILE"

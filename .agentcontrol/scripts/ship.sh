#!/usr/bin/env bash
set -Eeuo pipefail
IFS=$'\n\t'
LC_ALL=C.UTF-8

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "$SCRIPT_DIR/lib/common.sh"

sdk::load_commands

sdk::log "INF" "Запуск agentcall verify перед ship"
"$SDK_ROOT/scripts/verify.sh"

VERIFY_REPORT="$SDK_ROOT/reports/verify.json"
if [[ ! -f "$VERIFY_REPORT" ]]; then
  sdk::die "ship: отсутствует reports/verify.json после verify"
fi

python3 <<'PY' "$VERIFY_REPORT"
import json, sys, pathlib

report_path = pathlib.Path(sys.argv[1])
data = json.loads(report_path.read_text(encoding="utf-8"))
exit_code = int(data.get("exit_code", 0))
findings = len(data.get("quality", {}).get("findings", []))
failed = [step for step in data.get("steps", []) if step.get("status") == "fail"]
messages: list[str] = []
if exit_code != 0:
    messages.append(f"verify exit_code={exit_code}")
if failed:
    names = ", ".join(step.get("name", "?") for step in failed)
    messages.append(f"failed steps: {names}")
if findings:
    messages.append(f"quality_guard findings: {findings}")
if messages:
    print("ship: блокировано — " + "; ".join(messages))
    sys.exit(1)
sys.exit(0)
PY

sdk::run_command_group "ship" SDK_SHIP_COMMANDS

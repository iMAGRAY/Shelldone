#!/usr/bin/env bash
set -Eeuo pipefail
IFS=$'\n\t'
LC_ALL=C.UTF-8

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "$SCRIPT_DIR/lib/common.sh"

PIP_AUDIT="$SDK_ROOT/.venv/bin/pip-audit"
LOCK_FILE="$SDK_ROOT/requirements.lock"
REPORT="$SDK_ROOT/reports/pip-audit.json"
SARIF_REPORT="$SDK_ROOT/reports/pip-audit.sarif"
CACHE_DIR="$SDK_ROOT/.sdk/pip-audit-cache"

if [[ ! -x "$PIP_AUDIT" ]]; then
  sdk::die "scan-sbom: отсутствует pip-audit — выполните agentcall setup"
fi

if [[ ! -f "$LOCK_FILE" ]]; then
  sdk::die "scan-sbom: отсутствует requirements.lock — выполните agentcall lock"
fi

mkdir -p "$SDK_ROOT/reports"
mkdir -p "$CACHE_DIR"

set +e
"$PIP_AUDIT" --requirement "$LOCK_FILE" --format json --output "$REPORT" --cache-dir "$CACHE_DIR" --progress-spinner off
status_json=$?
set -e

if [[ $status_json -ne 0 ]]; then
  sdk::log "WRN" "pip-audit вернул код $status_json — анализирую отчёт"
fi

python3 - "$REPORT" "$SARIF_REPORT" <<'PY'
import json
import sys
from pathlib import Path

report_path = Path(sys.argv[1])
sarif_path = Path(sys.argv[2])
raw = report_path.read_text(encoding="utf-8")
if not raw.strip():
    data = {"dependencies": []}
else:
    data = json.loads(raw)

dependencies = data.get("dependencies", []) if isinstance(data, dict) else data
results = []
rules = {}

for entry in dependencies:
    vulns = entry.get("vulns") or entry.get("vulnerabilities") or []
    dep_name = entry.get("name", "unknown")
    dep_version = entry.get("version", "")
    for vuln in vulns:
        vuln_id = vuln.get("id") or f"{dep_name}-unknown"
        description = vuln.get("description") or "Vulnerability reported by pip-audit"
        severity = (vuln.get("severity") or "error").lower()
        references = vuln.get("references") or vuln.get("links") or []
        help_uri = references[0] if references else None

        if vuln_id not in rules:
            rule = {
                "id": vuln_id,
                "name": vuln_id,
                "shortDescription": {"text": description[:120]},
                "fullDescription": {"text": description},
                "helpUri": help_uri,
                "properties": {"problem.severity": severity}
            }
            rules[vuln_id] = rule

        message = f"{dep_name} {dep_version}: {description}"
        results.append({
            "ruleId": vuln_id,
            "level": "error",
            "message": {"text": message},
            "locations": [
                {
                    "physicalLocation": {
                        "artifactLocation": {"uri": "requirements.lock"}
                    }
                }
            ]
        })

sarif = {
    "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
    "version": "2.1.0",
    "runs": [
        {
            "tool": {
                "driver": {
                    "name": "pip-audit",
                    "informationUri": "https://github.com/pypa/pip-audit",
                    "rules": list(rules.values())
                }
            },
            "results": results
        }
    ]
}

sarif_path.write_text(json.dumps(sarif, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

if results:
    print("pip-audit обнаружил уязвимости:")
    for result in results:
        print(f"  - {result['ruleId']}: {result['message']['text']}")
    sys.exit(1)

print("pip-audit: уязвимости не найдены")
PY

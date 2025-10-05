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
VERIFY_JSON="$REPORT_DIR/verify.json"

declare -a VERIFY_STEPS
OVERALL_EXIT=0

record_step() {
  local name="$1" status="$2" exit_code="$3" log_path="$4" severity="$5" duration="$6"
  VERIFY_STEPS+=("$name|$status|$exit_code|$log_path|$severity|$duration")
  if [[ $status == "fail" && $severity == "critical" ]]; then
    OVERALL_EXIT=1
  fi
}

run_step() {
  local name="$1" severity="$2" cmd="$3"
  local log_file
  log_file="$(mktemp)"
  sdk::log "RUN" "$name"
  local start_ts
  start_ts=$(python3 - <<'PY'
import time
print(time.time())
PY
)
  set +e
  eval "$cmd" >"$log_file" 2>&1
  local exit_code=$?
  set -e
  local duration
  duration=$(START_TS="$start_ts" python3 - <<'PY'
import os, time
print(f"{time.time()-float(os.environ['START_TS']):.6f}")
PY
)
  if [[ $exit_code -eq 0 ]]; then
    sdk::log "INF" "$name: success"
    record_step "$name" "ok" "$exit_code" "$log_file" "$severity" "$duration"
  else
    sdk::log "WRN" "$name: exit $exit_code"
    record_step "$name" "fail" "$exit_code" "$log_file" "$severity" "$duration"
  fi
}

collect_log_tail() {
  local file="$1"
  if [[ -f "$file" ]]; then
    tail -n 120 "$file"
  else
    printf ""
  fi
}

run_step "sync-architecture" "critical" "\"$SDK_ROOT/scripts/sync-architecture.sh\""
run_step "architecture-integrity" "critical" "\"$SDK_ROOT/scripts/check-architecture-integrity.py\""
run_step "sync-roadmap" "warning" "\"$SDK_ROOT/scripts/sync-roadmap.sh\""

run_step "ensure:AGENTS.md" "critical" "( sdk::ensure_file 'AGENTS.md' )"
run_step "ensure:todo.machine.md" "critical" "( sdk::ensure_file 'todo.machine.md' )"
run_step "ensure:.editorconfig" "critical" "( sdk::ensure_editorconfig )"
run_step "ensure:.codexignore" "critical" "( sdk::ensure_codexignore )"
run_step "ensure:data/tasks.board.json" "critical" "( sdk::ensure_file 'data/tasks.board.json' )"

run_step "check:todo_sections" "critical" "grep -q '^## Program' \"$SDK_ROOT/todo.machine.md\" && grep -q '^## Epics' \"$SDK_ROOT/todo.machine.md\" && grep -q '^## Big Tasks' \"$SDK_ROOT/todo.machine.md\""

run_step "shellcheck" "warning" "sdk::run_shellcheck_if_available"
run_step "roadmap-status" "warning" "\"$SDK_ROOT/scripts/roadmap-status.sh\" compact"
run_step "task-validate" "warning" "\"$SDK_ROOT/scripts/task.sh\" validate"
run_step "heart-check" "warning" "\"$SDK_ROOT/scripts/agents/heart_check.sh\""

# quality guard (diff против базового коммита)
BASE_REF_DEFAULT="${VERIFY_BASE_REF:-origin/main}"
determine_base_commit() {
  local base_ref="$1"
  if git rev-parse --verify HEAD >/dev/null 2>&1; then
    if git rev-parse --verify "$base_ref" >/dev/null 2>&1; then
      git merge-base HEAD "$base_ref"
      return 0
    fi
    if git rev-parse --verify HEAD^ >/dev/null 2>&1; then
      git rev-parse HEAD^
      return 0
    fi
    git rev-parse HEAD
    return 0
  fi
  printf ""
}

BASE_COMMIT="${VERIFY_BASE_COMMIT:-$(determine_base_commit "$BASE_REF_DEFAULT")}" || true
QUALITY_JSON="$REPORT_DIR/verify_quality.json"
if [[ -n "$BASE_COMMIT" ]]; then
run_step "quality_guard" "warning" "python3 -m scripts.lib.quality_guard --base \"$BASE_COMMIT\" --include-untracked --output \"$QUALITY_JSON\""

run_step "check-lock" "critical" "\"$SDK_ROOT/scripts/check-lock.sh\""
run_step "scan-sbom" "critical" "\"$SDK_ROOT/scripts/scan-sbom.sh\""
else
  sdk::log "WRN" "Не удалось определить базовый коммит для quality_guard"
fi

# кастомные команды верификации (не прерывают скрипт)
if [[ ${#SDK_VERIFY_COMMANDS[@]} -eq 0 ]]; then
  sdk::log "INF" "SDK_VERIFY_COMMANDS пуст — пропуск"
else
  idx=0
  for cmd in "${SDK_VERIFY_COMMANDS[@]}"; do
    idx=$((idx + 1))
    run_step "verify_cmd[$idx]" "warning" "$cmd"
  done
fi

EXIT_ON_FAIL=${EXIT_ON_FAIL:-0}

declare -a steps_json
for entry in "${VERIFY_STEPS[@]}"; do
  IFS='|' read -r name status exit_code log_path severity duration <<<"$entry"
  LOG_CONTENT="$(collect_log_tail "$log_path" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))')"
  steps_json+=("{\"name\":\"$name\",\"status\":\"$status\",\"severity\":\"$severity\",\"exit_code\":$exit_code,\"duration_sec\":$duration,\"log_tail\":$LOG_CONTENT}")
done

QUALITY_REPORT="{}"
if [[ -f "$QUALITY_JSON" ]]; then
  QUALITY_REPORT=$(python3 -c 'import json,sys; print(json.dumps(json.load(open(sys.argv[1],encoding="utf-8"))))' "$QUALITY_JSON" 2>/dev/null || printf '{}')
fi

VERIFY_OUTPUT=$(cat <<JSON
{
  "generated_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "base": "$BASE_COMMIT",
  "steps": [$(IFS=,; printf '%s' "${steps_json[*]}")],
  "quality": $QUALITY_REPORT,
  "exit_code": $OVERALL_EXIT
}
JSON
)

printf '%s\n' "$VERIFY_OUTPUT" >"$VERIFY_JSON"
sdk::log "INF" "Отчёт сохранён: $VERIFY_JSON"

HAS_FINDINGS=0
if [[ -f "$QUALITY_JSON" ]]; then
  FINDINGS_COUNT=$(python3 -c 'import json,sys; data=json.load(open(sys.argv[1],encoding="utf-8")); print(len(data.get("findings",[])))' "$QUALITY_JSON" 2>/dev/null || printf 0)
  if [[ ${FINDINGS_COUNT:-0} -gt 0 ]]; then
    sdk::log "WRN" "quality_guard: обнаружено $FINDINGS_COUNT потенциальных проблем"
    HAS_FINDINGS=1
  fi
fi

if [[ $EXIT_ON_FAIL == 1 ]]; then
  if [[ $OVERALL_EXIT -ne 0 || $HAS_FINDINGS -eq 1 ]]; then
    sdk::die "verify: обнаружены критичные ошибки — см. $VERIFY_JSON"
  fi
else
  if [[ $OVERALL_EXIT -ne 0 ]]; then
    sdk::log "ERR" "Верификация завершена с ошибками"
    exit $OVERALL_EXIT
  fi
fi

if [[ $HAS_FINDINGS -eq 1 ]]; then
  sdk::log "WRN" "Верификация завершена с предупреждениями"
  exit 0
fi

sdk::log "INF" "Верификация завершена без критичных ошибок"
exit 0

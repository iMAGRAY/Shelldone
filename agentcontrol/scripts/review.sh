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
REVIEW_JSON="$REPORT_DIR/review.json"
REVIEW_CHANGED_FILES_FILE="$REPORT_DIR/review_changed_files.txt"

BASE_REF_DEFAULT="${REVIEW_BASE_REF:-origin/main}"

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

BASE_COMMIT="${REVIEW_BASE_COMMIT:-$(determine_base_commit "$BASE_REF_DEFAULT")}" || true
TARGET_COMMIT="${REVIEW_TARGET_COMMIT:-HEAD}"

if [[ -z "$BASE_COMMIT" ]]; then
  sdk::die "Не удалось определить базовый коммит для diff"
fi

sdk::log "INF" "Базовый коммит: $BASE_COMMIT"
sdk::log "INF" "Целевой коммит: $TARGET_COMMIT"

mapfile -t CHANGED_FILES < <( \
  { git diff --name-only "$BASE_COMMIT" || true; \
    git status --porcelain | awk '$1 == "??" {print substr($0,4)}'; \
  } | awk 'NF' | sort -u
)

if [[ ${#CHANGED_FILES[@]} -eq 0 ]]; then
  sdk::log "INF" "Изменённых файлов нет — ревью пропущено"
  printf '%s\n' >"$REVIEW_JSON" "$(printf '{"generated_at":"%s","base":"%s","target":"%s","changed_files":[],"steps":[],"quality":{},"exit_code":0}\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$BASE_COMMIT" "$TARGET_COMMIT")"
  exit 0
fi

printf "%s\n" "${CHANGED_FILES[@]}" >"$REVIEW_CHANGED_FILES_FILE"
REVIEW_CHANGED_FILES="$(printf '%s\n' "${CHANGED_FILES[@]}")"
export REVIEW_CHANGED_FILES
export REVIEW_CHANGED_FILES_PATH="$REVIEW_CHANGED_FILES_FILE"

sdk::log "INF" "Изменённые файлы: ${#CHANGED_FILES[@]}"

CHANGED_JSON=$(
  python3 - "$REVIEW_CHANGED_FILES_FILE" <<'PY'
import json
import sys
from pathlib import Path
path = Path(sys.argv[1])
files = [line.strip() for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]
print(json.dumps(files))
PY
)

record_step() {
  local name="$1"
  local status="$2"
  local exit_code="$3"
  local log_file="$4"
  local duration="$5"
  REVIEW_STEPS+=("$name|$status|$exit_code|$log_file|$duration")
}

run_command_capture() {
  local name="$1"; shift
  local cmd="$1"; shift || true
  local tmp_log
  tmp_log="$(mktemp)"
  local exit_code=0
  sdk::log "RUN" "$name: $cmd"
  local start_ts
  start_ts=$(python3 - <<'PY'
import time
print(time.time())
PY
)
  set +e
  eval "$cmd" >"$tmp_log" 2>&1
  exit_code=$?
  set -e
  local duration
  duration=$(START_TS="$start_ts" python3 - <<'PY'
import os, time
print(f"{time.time()-float(os.environ['START_TS']):.6f}")
PY
)
  if [[ $exit_code -eq 0 ]]; then
    sdk::log "INF" "$name: success"
    record_step "$name" "ok" "$exit_code" "$tmp_log" "$duration"
  else
    sdk::log "WRN" "$name: exit $exit_code"
    record_step "$name" "fail" "$exit_code" "$tmp_log" "$duration"
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

declare -a REVIEW_STEPS

# --- Quality scan ----------------------------------------------------------

QUALITY_JSON="$REPORT_DIR/review_quality.json"
sdk::log "INF" "Сканирование realness/secrets"
run_command_capture "quality_guard" "python3 -m scripts.lib.quality_guard --base \"$BASE_COMMIT\" --include-untracked --output \"$QUALITY_JSON\""

# --- Review linters -------------------------------------------------------

if [[ ${#SDK_REVIEW_LINTERS[@]} -eq 0 ]]; then
  sdk::log "INF" "SDK_REVIEW_LINTERS пуст — шаг пропущен"
else
  local_index=0
  for cmd in "${SDK_REVIEW_LINTERS[@]}"; do
    local_index=$((local_index + 1))
    run_command_capture "review_linter[$local_index]" "$cmd"
  done
fi

# --- Tests / coverage -----------------------------------------------------

if [[ -n "${SDK_TEST_COMMAND:-}" ]]; then
  run_command_capture "test" "$SDK_TEST_COMMAND"
else
  sdk::log "INF" "SDK_TEST_COMMAND не задан — тесты пропущены"
fi

# diff-cover (опционально)
DIFF_COVER_STATUS="skipped"
if [[ -n "${SDK_COVERAGE_FILE:-}" && -f "$SDK_COVERAGE_FILE" ]]; then
  if command -v diff-cover >/dev/null 2>&1; then
    DIFF_COVER_LOG="$(mktemp)"
    set +e
    diff-cover "$SDK_COVERAGE_FILE" --compare-branch "$BASE_COMMIT" >"$DIFF_COVER_LOG" 2>&1
    EXIT_CODE=$?
    set -e
    if [[ $EXIT_CODE -eq 0 ]]; then
      DIFF_COVER_STATUS="ok"
    else
      DIFF_COVER_STATUS="fail"
    fi
    record_step "diff-cover" "$DIFF_COVER_STATUS" "$EXIT_CODE" "$DIFF_COVER_LOG"
  else
    sdk::log "WRN" "diff-cover не найден — шаг пропущен"
  fi
fi

# --- Итоговый отчёт -------------------------------------------------------

EXIT_ON_FAIL=${EXIT_ON_FAIL:-0}
OVERALL_EXIT=0

declare -a steps_json
for entry in "${REVIEW_STEPS[@]}"; do
  IFS='|' read -r name status exit_code log_path duration <<<"$entry"
  LOG_CONTENT="$(collect_log_tail "$log_path" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))')"
  steps_json+=("{\"name\":\"$name\",\"status\":\"$status\",\"exit_code\":$exit_code,\"duration_sec\":$duration,\"log_tail\":$LOG_CONTENT}")
  if [[ $status == "fail" ]]; then
    OVERALL_EXIT=1
  fi
done

QUALITY_REPORT="{}"
if [[ -f "$QUALITY_JSON" ]]; then
  QUALITY_REPORT=$(python3 -c 'import json,sys; print(json.dumps(json.load(open(sys.argv[1],encoding="utf-8"))))' "$QUALITY_JSON" 2>/dev/null || printf '{}')
fi

REVIEW_OUTPUT=$(cat <<JSON
{
  "generated_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "base": "$BASE_COMMIT",
  "target": "$TARGET_COMMIT",
  "changed_files": $CHANGED_JSON,
  "steps": [$(IFS=,; printf '%s' "${steps_json[*]}")],
  "quality": $QUALITY_REPORT,
  "exit_code": $OVERALL_EXIT
}
JSON
)

printf '%s\n' "$REVIEW_OUTPUT" >"$REVIEW_JSON"
sdk::log "INF" "Отчёт сохранён: $REVIEW_JSON"

HAS_FINDINGS=0
if [[ -f "$QUALITY_JSON" ]]; then
  FINDINGS_COUNT=$(python3 -c 'import json,sys; data=json.load(open(sys.argv[1],encoding="utf-8")); print(len(data.get("findings",[])))' "$QUALITY_JSON" 2>/dev/null || printf 0)
  if [[ ${FINDINGS_COUNT:-0} -gt 0 ]]; then
    sdk::log "WRN" "Найдены потенциальные заглушки/секреты: $FINDINGS_COUNT"
    HAS_FINDINGS=1
  fi
fi

if [[ $EXIT_ON_FAIL == 1 ]]; then
  if [[ $OVERALL_EXIT -ne 0 || $HAS_FINDINGS -eq 1 ]]; then
    sdk::die "review: есть проблемы — см. $REVIEW_JSON"
  fi
fi

if [[ $OVERALL_EXIT -ne 0 || $HAS_FINDINGS -eq 1 ]]; then
  sdk::log "WRN" "Ревью завершено с предупреждениями; см. $REVIEW_JSON"
  exit 0
fi

sdk::log "INF" "Ревью завершено без критичных проблем"
exit 0

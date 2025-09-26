#!/usr/bin/env bash
set -Eeuo pipefail
IFS=$'\n\t'
LC_ALL="${LC_ALL:-C.UTF-8}"

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
VERIFY_MODE="${VERIFY_MODE:-prepush}"
JSON_OUTPUT="${JSON:-0}"
CHANGED_ONLY="${CHANGED_ONLY:-0}"
TIMEOUT_MIN="${TIMEOUT_MIN:-0}"
NET="${NET:-0}"

python3 "$ROOT_DIR/scripts/verify.py" \
  --mode "$VERIFY_MODE" \
  --json "$JSON_OUTPUT" \
  --changed-only "$CHANGED_ONLY" \
  --timeout-min "$TIMEOUT_MIN" \
  --net "$NET" \
  "$@"

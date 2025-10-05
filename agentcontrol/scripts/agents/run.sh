#!/usr/bin/env bash
set -Eeuo pipefail
IFS=$'\n\t'
LC_ALL=C.UTF-8

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PYTHON=${PYTHON:-python3}
COMMAND=${1:-help}
TASK=${2:-}
AGENT=${3:-codex}
ROLE=${4:-}
shift 4 || true

exec "$PYTHON" "$SCRIPT_DIR/run.py" "$COMMAND" "--task" "$TASK" "--agent" "$AGENT" "--role" "$ROLE" "$@"

#!/usr/bin/env bash
set -Eeuo pipefail
IFS=$'\n\t'
LC_ALL=C.UTF-8

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
PYTHON=${PYTHON:-python3}
COMMAND=${1:-help}
shift || true

exec "$PYTHON" "$SCRIPT_DIR/heart_engine.py" "$COMMAND" "$@"

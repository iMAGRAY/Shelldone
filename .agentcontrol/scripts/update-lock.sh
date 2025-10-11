#!/usr/bin/env bash
set -Eeuo pipefail
IFS=$'\n\t'
LC_ALL=C.UTF-8

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "$SCRIPT_DIR/lib/common.sh"

LOCK_SRC="$SDK_ROOT/requirements.txt"
LOCK_DST="$SDK_ROOT/requirements.lock"
SBOM_DST="$SDK_ROOT/sbom/python.json"

if [[ ! -f "$LOCK_SRC" ]]; then
  sdk::die "update-lock: отсутствует $LOCK_SRC"
fi

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT
VENV="$TMP_DIR/venv"

python3 -m venv "$VENV"
"$VENV/bin/pip" install --upgrade pip==24.2 >/dev/null
"$VENV/bin/pip" install --quiet pip-tools==7.4.1 >/dev/null

sdk::log "INF" "Генерирую requirements.lock через pip-compile"
("$VENV/bin/python" -m piptools compile \
  --quiet \
  --no-annotate \
  --no-header \
  --generate-hashes \
  --resolver=backtracking \
  --strip-extras \
  --output-file "$TMP_DIR/requirements.lock" \
  "$LOCK_SRC")

mv "$TMP_DIR/requirements.lock" "$LOCK_DST"

if [[ ! -x "$SDK_ROOT/.venv/bin/python" ]]; then
  sdk::die "update-lock: не найдено окружение .venv — выполните agentcall setup"
fi

sdk::log "INF" "Генерирую SBOM $SBOM_DST"
"$SDK_ROOT/.venv/bin/python" "$SCRIPT_DIR/generate-sbom.py" --output "$SBOM_DST"

sdk::log "INF" "Lock-файл и SBOM обновлены"

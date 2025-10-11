#!/usr/bin/env bash
set -Eeuo pipefail
IFS=$'\n\t'
LC_ALL=C.UTF-8

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
TARGET="${AGENTCONTROL_HOME:-$HOME/.agentcontrol}"
TEMPLATE_SRC="$ROOT/src/agentcontrol/templates/0.2.0/project"
TEMPLATE_DEST="$TARGET/templates/stable/0.2.0"

mkdir -p "$TEMPLATE_DEST"
rsync -a --delete "$TEMPLATE_SRC/" "$TEMPLATE_DEST/"

cat >"$TEMPLATE_DEST/template.sha256" <<'HASH'
placeholder
HASH

echo "Templates staged to $TEMPLATE_DEST"

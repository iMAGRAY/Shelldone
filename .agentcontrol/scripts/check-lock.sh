#!/usr/bin/env bash
set -Eeuo pipefail
IFS=$'\n\t'
LC_ALL=C.UTF-8

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "$SCRIPT_DIR/lib/common.sh"

VENV_PIP="$SDK_ROOT/.venv/bin/pip"
LOCK_SRC="$SDK_ROOT/requirements.txt"
LOCK_FILE="$SDK_ROOT/requirements.lock"

if [[ ! -x "$VENV_PIP" ]]; then
  sdk::die "check-lock: отсутствует $VENV_PIP — выполните agentcall setup"
fi

if [[ ! -f "$LOCK_FILE" ]]; then
  sdk::die "check-lock: отсутствует requirements.lock"
fi

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

TMP_ENV="$TMP_DIR/venv"
python3 -m venv "$TMP_ENV"
"$TMP_ENV/bin/pip" install --upgrade pip==24.2 >/dev/null
"$TMP_ENV/bin/pip" install --quiet pip-tools==7.4.1 >/dev/null

COMPILED_HASH="$TMP_DIR/requirements.lock"
COMPILED_PLAIN="$TMP_DIR/requirements.nohash"

"$TMP_ENV/bin/python" -m piptools compile \
  --quiet \
  --no-annotate \
  --no-header \
  --generate-hashes \
  --resolver=backtracking \
  --strip-extras \
  --output-file "$COMPILED_HASH" \
  "$LOCK_SRC"

"$TMP_ENV/bin/python" -m piptools compile \
  --quiet \
  --no-annotate \
  --no-header \
  --resolver=backtracking \
  --strip-extras \
  --output-file "$COMPILED_PLAIN" \
  "$LOCK_SRC"

if ! cmp -s "$LOCK_FILE" "$COMPILED_HASH"; then
  sdk::log "ERR" "requirements.lock рассинхронизирован"
  diff -u "$LOCK_FILE" "$COMPILED_HASH" || true
  sdk::die "Обновите lock-файл: scripts/update-lock.sh"
fi

TMP_FREEZE="$(mktemp)"
TMP_LOCK_NORMALIZED="$(mktemp)"

"$VENV_PIP" freeze --exclude-editable --disable-pip-version-check | LC_ALL=C sort > "$TMP_FREEZE"
python3 - <<'PY' "$TMP_FREEZE"
import sys
from pathlib import Path

freeze_path = Path(sys.argv[1])
IGNORED = {"pip", "setuptools", "wheel", "pkg-resources", "distribute"}
entries = []
for raw in freeze_path.read_text(encoding="utf-8").splitlines():
    stripped = raw.strip()
    if not stripped or stripped.startswith("#"):
        continue
    if "==" in stripped:
        name, version = stripped.split("==", 1)
        base = name.strip()
        if base.lower() in IGNORED:
            continue
        norm = base.replace("_", "-").replace(".", "-").lower()
        entry = f"{norm}=={version.strip()}"
    elif " @ " in stripped:
        name, rest = stripped.split(" @ ", 1)
        base = name.strip()
        if base.lower() in IGNORED:
            continue
        norm = base.replace("_", "-").replace(".", "-").lower()
        entry = f"{norm} @ {rest.strip()}"
    else:
        base = stripped
        if base.lower() in IGNORED:
            continue
        entry = base.replace("_", "-").replace(".", "-").lower()
    entries.append(entry)
entries.sort()
freeze_path.write_text("\n".join(entries) + "\n", encoding="utf-8")
PY

python3 - <<'PY' "$COMPILED_PLAIN" "$TMP_LOCK_NORMALIZED"
import sys
from pathlib import Path

source = Path(sys.argv[1])
dest = Path(sys.argv[2])
IGNORED = {"pip", "setuptools", "wheel", "pkg-resources", "distribute"}
entries = []
for raw in source.read_text(encoding="utf-8").splitlines():
    stripped = raw.strip()
    if not stripped or stripped.startswith("#"):
        continue
    if "==" in stripped:
        name, version = stripped.split("==", 1)
        base = name.strip()
        if base.lower() in IGNORED:
            continue
        norm = base.replace("_", "-").replace(".", "-").lower()
        entry = f"{norm}=={version.strip()}"
    elif " @ " in stripped:
        name, rest = stripped.split(" @ ", 1)
        base = name.strip()
        if base.lower() in IGNORED:
            continue
        norm = base.replace("_", "-").replace(".", "-").lower()
        entry = f"{norm} @ {rest.strip()}"
    else:
        base = stripped
        if base.lower() in IGNORED:
            continue
        entry = base.replace("_", "-").replace(".", "-").lower()
    entries.append(entry)
entries.sort()
dest.write_text("\n".join(entries) + "\n", encoding="utf-8")
PY

if ! cmp -s "$TMP_FREEZE" "$TMP_LOCK_NORMALIZED"; then
  sdk::log "ERR" "Установленные зависимости не соответствуют requirements.lock"
  diff -u "$TMP_LOCK_NORMALIZED" "$TMP_FREEZE" || true
  sdk::die "Выполните agentcall setup или scripts/update-lock.sh"
fi

sdk::log "INF" "requirements.lock валиден и окружение синхронно"

SBOM_PATH="$SDK_ROOT/sbom/python.json"
if [[ -f "$SBOM_PATH" ]]; then
  if ! "$SDK_ROOT/.venv/bin/python" "$SCRIPT_DIR/generate-sbom.py" --check --output "$SBOM_PATH"; then
    sdk::die "SBOM не соответствует текущей среде — запустите scripts/update-lock.sh"
  fi
else
  sdk::log "WRN" "SBOM отсутствует (sbom/python.json)"
fi

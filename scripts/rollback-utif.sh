#!/usr/bin/env bash
set -euo pipefail

TAG="codex/2025-10-03-pre-utif"

if ! git rev-parse "$TAG" >/dev/null 2>&1; then
  echo "[rollback-utif] Tag $TAG is missing. Aborting." >&2
  exit 1
fi

echo "[rollback-utif] This will checkout $TAG in detached mode."
echo "[rollback-utif] Export ROLLBACK_CONFIRM=yes to proceed."

if [[ "${ROLLBACK_CONFIRM:-}" != "yes" ]]; then
  exit 2
fi

git switch --detach "$TAG"
echo "[rollback-utif] Repository now at $TAG. Verify and create new branch if needed."

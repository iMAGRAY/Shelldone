#!/usr/bin/env bash
set -euo pipefail

TAG="codex/2025-10-03-pre-ack"

if ! git rev-parse "$TAG" >/dev/null 2>&1; then
  echo "[rollback-ack] Tag $TAG is missing. Aborting." >&2
  exit 1
fi

echo "[rollback-ack] This will checkout $TAG in detached mode."
echo "[rollback-ack] Export ROLLBACK_CONFIRM=yes to proceed."

if [[ "${ROLLBACK_CONFIRM:-}" != "yes" ]]; then
  exit 2
fi

git switch --detach "$TAG"
echo "[rollback-ack] Repository now at $TAG. Verify and create new branch if needed."

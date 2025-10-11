#!/usr/bin/env bash
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"

log() {
  printf '[review] %s\n' "$*"
}

run() {
  local step="$1"
  shift
  log "start ${step}"
  if "$@"; then
    log "ok ${step}"
  else
    log "fail ${step}"
    return 1
  fi
}

main() {
  cd "${ROOT}"

  run fmt-check cargo +nightly fmt --all -- --check
  run verify env VERIFY_MODE=prepush JSON=0 "${ROOT}/scripts/verify.sh"
  run race cargo test -p shelldone-agentd --tests -- --nocapture
  run e2e cargo test -p shelldone-agentd --test e2e_ack
  run dup python3 "${ROOT}/scripts/check_duplication.py"
  run contracts python3 "${ROOT}/scripts/check_contracts.py"
  run sbom python3 "${ROOT}/scripts/generate_sbom.py"

  log "review sequence complete"
}

main "$@"

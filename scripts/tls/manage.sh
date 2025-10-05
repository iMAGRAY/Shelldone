#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/tls/manage.sh <command> [options]

Commands:
  status                       Print TLS certificate fingerprint, expiry, and active policy.
  issue  --cn <CN> [--out DIR] [--days N]
                               Generate self-signed CA + server certificate pair.
  rotate --cert FILE --key FILE [--ca FILE] [--dest DIR]
                               Atomically rotate TLS materials, keeping a timestamped backup.
  help                         Show this message.

Environment:
  SHELLDONE_STATUS_ENDPOINT    Override status endpoint (default http://127.0.0.1:17717/status).
  SHELLDONE_TLS_DIR            Destination directory for TLS assets (default state/tls).
USAGE
}

fail() {
  echo "[tls/manage] $*" >&2
  exit 1
}

root_dir() {
  local script_dir
  script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
  echo "$script_dir"
}

ensure_tools() {
  for tool in openssl; do
    command -v "$tool" >/dev/null 2>&1 || fail "Required tool '$tool' is not installed."
  done
}

print_status() {
  local root tls_dir cert ca key endpoint policy_json policy
  root=$(root_dir)
  tls_dir=${SHELLDONE_TLS_DIR:-"$root/state/tls"}
  cert="$tls_dir/cert.pem"
  key="$tls_dir/key.pem"
  ca="$tls_dir/ca.pem"
  if [[ ! -f "$cert" || ! -f "$key" ]]; then
    fail "TLS materials not found in $tls_dir (expected cert.pem and key.pem)."
  fi
  echo "TLS directory: $tls_dir"
  echo "Fingerprint (SHA-256):"
  openssl x509 -noout -fingerprint -sha256 -in "$cert"
  echo "Validity:"
  openssl x509 -noout -dates -in "$cert"
  echo "Subject:"
  openssl x509 -noout -subject -in "$cert"
  if [[ -f "$ca" ]]; then
    echo "CA Fingerprint (SHA-256):"
    openssl x509 -noout -fingerprint -sha256 -in "$ca"
  fi
  endpoint=${SHELLDONE_STATUS_ENDPOINT:-"http://127.0.0.1:17717/status"}
  if command -v curl >/dev/null 2>&1; then
    policy_json=$(curl -sSf --max-time 2 "$endpoint" 2>/dev/null || true)
    if [[ -n "$policy_json" ]]; then
      if command -v jq >/dev/null 2>&1; then
        policy=$(printf '%s' "$policy_json" | jq -r '.tls.policy // "unknown"')
      else
        policy=$(python3 -c 'import json,sys;print(json.load(sys.stdin).get("tls",{}).get("policy","unknown"))' <<<"$policy_json" 2>/dev/null || echo "unknown")
      fi
      echo "Active policy (from $endpoint): $policy"
    else
      echo "Active policy: unknown (status endpoint unavailable)"
    fi
  else
    echo "Active policy: unknown (curl missing)"
  fi
}

issue_materials() {
  local root tls_dir cn days tmp ca_key ca_cert server_key server_csr server_cert
  cn=""
  tls_dir=""
  days=365
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --cn)
        cn="$2"; shift 2 ;;
      --out)
        tls_dir="$2"; shift 2 ;;
      --days)
        days="$2"; shift 2 ;;
      *)
        fail "Unknown option for issue: $1" ;;
    esac
  done
  [[ -n "$cn" ]] || fail "--cn is required for issue"
  root=$(root_dir)
  tls_dir=${tls_dir:-"$root/state/tls"}
  mkdir -p "$tls_dir"
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT
  ca_key="$tmp/ca.key"
  ca_cert="$tmp/ca.pem"
  server_key="$tmp/server.key"
  server_csr="$tmp/server.csr"
  server_cert="$tmp/server.pem"
  openssl req -x509 -nodes -newkey rsa:4096 -keyout "$ca_key" -out "$ca_cert" -days "$days" \
    -subj "/CN=$cn CA" -sha256 >/dev/null 2>&1
  openssl req -nodes -newkey rsa:4096 -keyout "$server_key" -out "$server_csr" \
    -subj "/CN=$cn" -sha256 >/dev/null 2>&1
  openssl x509 -req -in "$server_csr" -out "$server_cert" -sha256 -days "$days" \
    -CA "$ca_cert" -CAkey "$ca_key" -CAcreateserial \
    -extfile <(printf 'subjectAltName=DNS:%s\nextendedKeyUsage=serverAuth,clientAuth\n' "$cn") >/dev/null 2>&1
  install -m 0600 "$server_key" "$tls_dir/key.pem"
  install -m 0644 "$server_cert" "$tls_dir/cert.pem"
  install -m 0644 "$ca_cert" "$tls_dir/ca.pem"
  install -m 0600 "$ca_key" "$tls_dir/ca.key"
  rm -f "$tls_dir/ca.pem.srl" "$tls_dir/cert.pem.srl"
  echo "Issued new TLS materials for CN=$cn in $tls_dir"
}

rotate_materials() {
  local root tls_dir cert key ca dest backup timestamp
  root=$(root_dir)
  tls_dir=${SHELLDONE_TLS_DIR:-"$root/state/tls"}
  dest="$tls_dir"
  cert=""
  key=""
  ca=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --cert)
        cert="$2"; shift 2 ;;
      --key)
        key="$2"; shift 2 ;;
      --ca)
        ca="$2"; shift 2 ;;
      --dest)
        dest="$2"; shift 2 ;;
      *)
        fail "Unknown option for rotate: $1" ;;
    esac
  done
  [[ -f "$cert" ]] || fail "--cert file not found"
  [[ -f "$key" ]] || fail "--key file not found"
  mkdir -p "$dest"
  timestamp=$(date -u +%Y%m%dT%H%M%SZ)
  backup="$dest/backups/$timestamp"
  mkdir -p "$backup"
  if [[ -f "$dest/cert.pem" ]]; then cp "$dest/cert.pem" "$backup/cert.pem"; fi
  if [[ -f "$dest/key.pem" ]]; then cp "$dest/key.pem" "$backup/key.pem"; fi
  if [[ -n "$ca" && -f "$dest/ca.pem" ]]; then cp "$dest/ca.pem" "$backup/ca.pem"; fi
  install -m 0644 "$cert" "$dest/cert.pem"
  install -m 0600 "$key" "$dest/key.pem"
  if [[ -n "$ca" ]]; then
    install -m 0644 "$ca" "$dest/ca.pem"
  fi
  echo "Rotated TLS materials into $dest (backup $backup)"
}

main() {
  ensure_tools
  local cmd
  cmd=${1:-help}
  shift || true
  case "$cmd" in
    status)
      print_status "$@" ;;
    issue)
      issue_materials "$@" ;;
    rotate)
      rotate_materials "$@" ;;
    help|-h|--help)
      usage ;;
    *)
      usage
      exit 1 ;;
  esac
}

main "$@"

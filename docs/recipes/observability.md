# Observability Operations Playbook

## Scope
This playbook explains how to provision Shelldone telemetry end-to-end, respond to the new TermBridge discovery alert, and verify that hardened HTTP fetch paths stay healthy. It targets operators and on-call engineers.

## 1. Prerequisites
- OTLP collector reachable at `$OTEL_EXPORTER_OTLP_ENDPOINT` (default `http://localhost:4318`).
- `python3 scripts/verify.py` succeeds in `VERIFY_MODE=prepush` (ensures metrics schema + alert definitions are in sync).
- `SHELLDONE_TERMBRIDGE_DISCOVERY_TOKEN` stored in the secrets backend or OS keychain (used by `agentd`).
- GUI launch profile updated with matching `SHELLDONE_AGENTD_DISCOVERY_TOKEN` in `config/experience.env`.

## 2. Configure Discovery Auth
1. Generate a 32+ byte random token (`openssl rand -hex 24`).
2. Set it for the daemon: `export SHELLDONE_TERMBRIDGE_DISCOVERY_TOKEN=$(cat state/secrets/discovery.token)`.
3. Mirror the token for GUI clients (Workbench, Experience Hub):
   ```bash
   echo "SHELLDONE_AGENTD_DISCOVERY_TOKEN=$(cat state/secrets/discovery.token)" >> config/experience.env
   ```
4. Enforce HTTPS: ensure `discovery.endpoints.http.listen` in `state/discovery/agentd.json` includes `https://`. Only for diagnostics set `SHELLDONE_GUI_ALLOW_INSECURE_AGENTD=1` (local machines, never production).
5. Reload the GUI. The telemetry overlay should log `experience.telemetry: discovery endpoint upgraded to https://…` once.

## 3. Alert Wiring (`termbridge.actions{command="discover"}`)
| Step | Action | Expected Result |
|------|--------|-----------------|
| 1 | Import `observability/termbridge_discover.alert.jsonnet` into Grafana | Alert rule visible under "TermBridge Ops" folder. |
| 2 | Set threshold `>150/min for 5m`; route to PagerDuty service `OPS::Agent`. | Alert summary shows correct severity (P2). |
| 3 | Add annotation link to this playbook section. | On-call can jump here directly. |

**Runbook (triggered alert):**
1. Check agent logs for `termbridge.service: discover backpressure=…` and queued requests.
2. Run `see docs/status.md --json | jq '.reports.termbridge.discover.backpressure'` (ensures queue depth).
3. If overload confirmed, scale CLI discovery interval: export `SHELLDONE_TERMBRIDGE_DISCOVER_BACKOFF_MS=500` and restart agentd.
4. Acknowledge the alert only after latency returns <150/min for 10 min; log incident in Continuum (`shelldone agent journal emit --kind termbridge.discovery.alert`).

## 4. Verification Checklist
- `cargo test -p shelldone-agentd termbridge` → covers auth enforcement (new `termbridge_discover_requires_token_when_configured`).
- `cargo test -p shelldone-gui -- lib::experience::adapters::agentd` → validates HTTPS upgrades, retries, and Bearer token propagation.
- `scripts/verify.py --json` confirms `observability-alert` check passes (document entry present).
- `python3 scripts/tests/termbridge_matrix.py [--emit-otlp]` → генерирует Capability Map через `shelldone-agentd --termbridge-export` и проверяет, что diff/latency соответствуют бюджету (<200 мс).
- Manually hit the endpoint:
  ```bash
  curl -sk -H "Authorization: Bearer $SHELLDONE_TERMBRIDGE_DISCOVERY_TOKEN" \
       https://127.0.0.1:17719/termbridge/discover
  ```
  Expect HTTP 200; without header expect HTTP 401.

## 5. Troubleshooting
- **401 Unauthorized:** tokens mismatch or GUI restarted without new env. Sync the secret, restart GUI, and watch for log `termbridge discover … rejected with 401` disappearing.
- **HTTP fallback denied:** set `SHELLDONE_GUI_ALLOW_INSECURE_AGENTD=1` temporarily (non-prod). Logs warn once per boot; remove the variable afterwards.
- **Repeated retry exhaustion:** check network/firewall reachability; GUI emits debug log with final failure reason. Use `openssl s_client -connect host:port` to confirm TLS handshake.

## 6. Operational Notes
- Tokens should rotate every 30 days; document rotation via `docs/architecture/security-and-secrets.md` rotation table.
- Store alert evidence in `reports/observability/termbridge_discover/<timestamp>.md` (create directory if missing) for post-incident reviews.
- When Heart index is older than 6 h, run `python3 scripts/agentd.py heart . sync` before investigating telemetry gaps.

## 7. Related References
- `docs/architecture/observability.md` — metrics inventory and dashboard catalogue.
- `docs/architecture/termbridge.md` — capability map, policy flow, and discovery semantics.
- `docs/architecture/agent-governance.md` → справочник по управлению адаптерами и troubleshooting.

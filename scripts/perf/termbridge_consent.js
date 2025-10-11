import http from 'k6/http';
import { Trend, Rate } from 'k6/metrics';

function envNumber(key, fallback, allowZero = false) {
  const raw = __ENV[key];
  if (raw === undefined) return fallback;
  const parsed = Number(raw);
  if (!Number.isFinite(parsed)) return fallback;
  if (!allowZero && parsed <= 0) return fallback;
  if (allowZero && parsed < 0) return fallback;
  return parsed;
}

function envString(key, fallback) {
  const raw = __ENV[key];
  if (raw === undefined || raw === '') return fallback;
  return raw;
}

const rate = envNumber('SHELLDONE_PERF_CONSENT_RATE', envNumber('SHELLDONE_PERF_RATE', 80));
const duration = envString('SHELLDONE_PERF_CONSENT_DURATION', envString('SHELLDONE_PERF_DURATION', '30s'));
const preAllocatedVUs = envNumber('SHELLDONE_PERF_CONSENT_VUS', envNumber('SHELLDONE_PERF_VUS', 20, true), true);
const maxVUs = envNumber('SHELLDONE_PERF_CONSENT_MAX_VUS', envNumber('SHELLDONE_PERF_MAX_VUS', 40, true), true);
const warmupSeconds = envNumber('SHELLDONE_PERF_CONSENT_WARMUP_SEC', envNumber('SHELLDONE_PERF_WARMUP_SEC', 0, true), true);
const baseUrl = envString('SHELLDONE_AGENTD_HOST', 'http://localhost:17717');

const latency = new Trend('termbridge_consent_latency');
const errors = new Rate('termbridge_consent_errors');

export const options = {
  scenarios: {
    constant_rate: {
      executor: 'constant-arrival-rate',
      rate,
      timeUnit: '1s',
      duration,
      preAllocatedVUs,
      maxVUs,
      startTime: `${warmupSeconds}s`,
    },
  },
  thresholds: {
    termbridge_consent_latency: ['p(95)<=50', 'p(99)<=100'],
    termbridge_consent_errors: ['rate<0.005'],
  },
};

function pickOptInTerminal() {
  try {
    const res = http.post(`${baseUrl}/termbridge/discover`, null, { timeout: '2s' });
    if (res.status !== 200) return 'iterm2';
    const body = res.json();
    if (!body || !body.terminals) return 'iterm2';
    const optins = body.terminals.filter((t) => t.requires_opt_in === true);
    if (optins.length > 0) return optins[0].terminal;
  } catch (_) {
    // ignore
  }
  return 'iterm2';
}

const TARGET_TERMINAL = envString('SHELLDONE_PERF_CONSENT_TERMINAL', pickOptInTerminal());

export default function () {
  // Alternate grant and revoke to exercise file IO path atomically
  const start = Date.now();
  const grantBody = JSON.stringify({ terminal: TARGET_TERMINAL });
  const grant = http.post(`${baseUrl}/termbridge/consent/grant`, grantBody, {
    headers: { 'Content-Type': 'application/json' },
    timeout: '1s',
  });
  if (grant.status !== 200) {
    errors.add(1);
    return;
  }
  const revoke = http.post(`${baseUrl}/termbridge/consent/revoke`, grantBody, {
    headers: { 'Content-Type': 'application/json' },
    timeout: '1s',
  });
  const elapsed = Date.now() - start;
  latency.add(elapsed);
  errors.add(revoke.status !== 200 ? 1 : 0);
}


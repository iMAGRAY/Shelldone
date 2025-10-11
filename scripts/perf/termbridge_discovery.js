import http from 'k6/http';
import { Trend, Rate } from 'k6/metrics';

function envNumber(key, fallback, allowZero = false) {
  const raw = __ENV[key];
  if (raw === undefined) {
    return fallback;
  }
  const parsed = Number(raw);
  if (!Number.isFinite(parsed)) {
    return fallback;
  }
  if (!allowZero && parsed <= 0) {
    return fallback;
  }
  if (allowZero && parsed < 0) {
    return fallback;
  }
  return parsed;
}

function envString(key, fallback) {
  const raw = __ENV[key];
  if (raw === undefined || raw === '') {
    return fallback;
  }
  return raw;
}

const rate = envNumber('SHELLDONE_PERF_TERMBRIDGE_RATE', envNumber('SHELLDONE_PERF_RATE', 80));
const duration = envString(
  'SHELLDONE_PERF_TERMBRIDGE_DURATION',
  envString('SHELLDONE_PERF_DURATION', '30s'),
);
const preAllocatedVUs = envNumber(
  'SHELLDONE_PERF_TERMBRIDGE_VUS',
  envNumber('SHELLDONE_PERF_VUS', 20, true),
  true,
);
const maxVUs = envNumber(
  'SHELLDONE_PERF_TERMBRIDGE_MAX_VUS',
  envNumber('SHELLDONE_PERF_MAX_VUS', 40, true),
  true,
);
const warmupSeconds = envNumber(
  'SHELLDONE_PERF_TERMBRIDGE_WARMUP_SEC',
  envNumber('SHELLDONE_PERF_WARMUP_SEC', 0, true),
  true,
);

const latency = new Trend('termbridge_discovery_latency');
const errors = new Rate('termbridge_discovery_errors');
const endpoint = envString(
  'SHELLDONE_PERF_TERMBRIDGE_ENDPOINT',
  'http://127.0.0.1:17717/termbridge/discover',
);
const bearerToken = envString('SHELLDONE_PERF_TERMBRIDGE_TOKEN', '');

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
    termbridge_discovery_latency: ['p(95)<=200', 'p(99)<=300'],
    termbridge_discovery_errors: ['rate<0.005'],
  },
};

export default function () {
  const start = Date.now();
  const headers = { 'Content-Type': 'application/json' };
  if (bearerToken) {
    headers.Authorization = `Bearer ${bearerToken}`;
  }
  const response = http.post(endpoint, null, {
    headers,
    timeout: '2s',
  });
  const elapsed = Date.now() - start;
  latency.add(elapsed);

  if (response.status !== 200) {
    errors.add(1);
    return;
  }

  const body = response.json();
  if (!body || typeof body !== 'object' || !('terminals' in body)) {
    errors.add(1);
    return;
  }

  errors.add(0);
}

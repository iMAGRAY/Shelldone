// k6 probe for Experience Hub telemetry fetch pipeline
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

const rate = envNumber('SHELLDONE_PERF_EXPERIENCE_RATE', envNumber('SHELLDONE_PERF_RATE', 80));
const duration = __ENV.SHELLDONE_PERF_EXPERIENCE_DURATION || __ENV.SHELLDONE_PERF_DURATION || '30s';
const preAllocatedVUs = envNumber('SHELLDONE_PERF_EXPERIENCE_VUS', envNumber('SHELLDONE_PERF_VUS', 30), true);
const maxVUs = envNumber('SHELLDONE_PERF_EXPERIENCE_MAX_VUS', envNumber('SHELLDONE_PERF_MAX_VUS', 60), true);
const warmupSeconds = envNumber('SHELLDONE_PERF_EXPERIENCE_WARMUP_SEC', envNumber('SHELLDONE_PERF_WARMUP_SEC', 0), true);

const telemetryLatency = new Trend('experience_hub_telemetry_latency');
const approvalsLatency = new Trend('experience_hub_approvals_latency');
const errors = new Rate('experience_hub_errors');

const HOST = __ENV.SHELLDONE_AGENTD_HOST || 'http://127.0.0.1:17717';

export const options = {
  scenarios: {
    experience_hub: {
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
    experience_hub_telemetry_latency: ['p(95)<=40', 'p(99)<=55'],
    experience_hub_approvals_latency: ['p(95)<=30', 'p(99)<=45'],
    experience_hub_errors: ['rate<0.01'],
  },
};

export default function () {
  const telemetryStart = Date.now();
  const telemetryResp = http.get(`${HOST}/context/full`, { timeout: '2s' });
  telemetryLatency.add(Date.now() - telemetryStart);
  if (telemetryResp.status !== 200) {
    errors.add(1);
  }

  const approvalsStart = Date.now();
  const approvalsResp = http.get(`${HOST}/approvals/pending`, { timeout: '2s' });
  approvalsLatency.add(Date.now() - approvalsStart);
  if (approvalsResp.status !== 200) {
    errors.add(1);
  }
}

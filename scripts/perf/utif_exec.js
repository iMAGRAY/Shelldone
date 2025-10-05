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

const rate = envNumber('SHELLDONE_PERF_RATE', 200);
const duration = __ENV.SHELLDONE_PERF_DURATION || '60s';
const preAllocatedVUs = envNumber('SHELLDONE_PERF_VUS', 50, true);
const maxVUs = envNumber('SHELLDONE_PERF_MAX_VUS', 100, true);
const warmupSeconds = envNumber('SHELLDONE_PERF_WARMUP_SEC', 0, true);

const latency = new Trend('utif_exec_latency');
const errors = new Rate('utif_exec_errors');

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
    utif_exec_latency: ['p(95)<=15', 'p(99)<=25'],
    utif_exec_errors: ['rate<0.005'],
  },
};

export default function () {
  const handshakePayload = JSON.stringify({
    version: 1,
    capabilities: { keyboard: ['kitty'], osc8: true },
  });
  const execPayload = JSON.stringify({
    command: 'agent.exec',
    persona: 'core',
    args: { cmd: 'echo hello_k6_perf' },
  });

  const hand = http.post('http://localhost:17717/sigma/handshake', handshakePayload, {
    headers: { 'Content-Type': 'application/json' },
    timeout: '2s',
  });

  if (hand.status !== 200) {
    errors.add(1);
    return;
  }

  const start = Date.now();
  const exec = http.post('http://localhost:17717/ack/exec', execPayload, {
    headers: { 'Content-Type': 'application/json' },
    timeout: '2s',
  });
  const elapsed = Date.now() - start;

  latency.add(elapsed);
  errors.add(exec.status !== 200 ? 1 : 0);
}

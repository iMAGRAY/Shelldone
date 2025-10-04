// k6 performance test for policy enforcement overhead
import http from 'k6/http';
import { Trend, Rate } from 'k6/metrics';

const latency_allowed = new Trend('policy_allowed_latency');
const latency_denied = new Trend('policy_denied_latency');
const errors = new Rate('policy_errors');

export const options = {
  scenarios: {
    policy_mix: {
      executor: 'constant-arrival-rate',
      rate: 100,
      timeUnit: '1s',
      duration: '30s',
      preAllocatedVUs: 20,
      maxVUs: 50,
    },
  },
  thresholds: {
    policy_allowed_latency: ['p(95)<=20', 'p(99)<=30'],
    policy_denied_latency: ['p(95)<=10', 'p(99)<=15'],
    policy_errors: ['rate<0.01'],
  },
};

export default function () {
  // 50% allowed (core persona), 50% denied (unknown persona)
  const persona = Math.random() < 0.5 ? 'core' : 'unknown_blocked';

  const payload = JSON.stringify({
    command: 'agent.exec',
    persona: persona,
    args: { cmd: 'echo test' },
  });

  const start = Date.now();
  const resp = http.post('http://localhost:17717/ack/exec', payload, {
    headers: { 'Content-Type': 'application/json' },
    timeout: '2s',
  });
  const elapsed = Date.now() - start;

  if (persona === 'core') {
    if (resp.status === 200) {
      latency_allowed.add(elapsed);
    } else {
      errors.add(1);
    }
  } else {
    if (resp.status === 403) {
      latency_denied.add(elapsed);
    } else {
      errors.add(1);
    }
  }
}

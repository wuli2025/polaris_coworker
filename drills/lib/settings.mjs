import { join } from 'node:path';
import { Blocked, json, repo } from './harness.mjs';
const shape = {
  host: { pingIntervalMs: [1, 60000], idleMs: [10000, 86400000], failureThreshold: [2, 20],
    stableResetMs: [60000, 86400000], restartBaseMs: [1, 30000], restartMaxMs: [1, 60000], restartJitterMs: [0, 10000] },
  sandbox: { memoryBytes: [6291456, 4294967296], cpus: [1, 2] },
  interconnect: { pollSeconds: [1, 60], maxAgeSeconds: [1, 600], approvalSeconds: [10, 300],
    windowHours: [1, 168], loginRequests: [10, 100], caddyImage: 'image' },
};
export function validateSettings(value) {
  if (!value || typeof value !== 'object' || Object.keys(value).some(k => !shape[k])) throw new Blocked('Unknown runtime settings group');
  for (const [group, fields] of Object.entries(shape)) {
    const v = value[group];
    if (!v || typeof v !== 'object' || Object.keys(v).some(k => !fields[k])) throw new Blocked(`Invalid runtime group ${group}`);
    for (const [key, rule] of Object.entries(fields)) {
      const n = v[key];
      if (rule === 'image') {
        if (typeof n !== 'string' || !/^[a-z0-9./_-]+:[a-zA-Z0-9_.-]+$/.test(n)
          || /:(latest|main|stable)$/.test(n)) throw new Blocked(`${group}.${key} requires a versioned image tag`);
      } else if (!Number.isSafeInteger(n) || n < rule[0] || n > rule[1]) {
        throw new Blocked(`${group}.${key} must be an integer ${rule[0]}..${rule[1]}`);
      }
    }
  }
  if (value.host.restartMaxMs < value.host.restartBaseMs || value.interconnect.maxAgeSeconds < value.interconnect.pollSeconds) {
    throw new Blocked('Inconsistent restart or revocation bounds');
  }
  return value;
}
export async function settings(h) {
  if (!h.settings) {
    let value;
    try { value = await json(process.env.DRILL_SETTINGS ?? join(repo, 'drills/config/runtime.json')); }
    catch { throw new Blocked('Runtime settings file missing or invalid JSON'); }
    h.settings = validateSettings(value);
  }
  return h.settings;
}

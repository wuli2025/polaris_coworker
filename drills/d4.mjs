import { join } from 'node:path';
import { readFile } from 'node:fs/promises';
import { run, check, waitFor, Blocked } from './lib/harness.mjs';
import { command } from './lib/process.mjs';
import { topology, compose, fixture, accountCall, login, delegatedBlocked } from './lib/d4-topology.mjs';
async function pair(h) {
  if (!h.pair) h.pair = (async () => ({ old: await login(h, 'Old device'), fresh: await login(h, 'New device') }))();
  return h.pair;
}
const steps = {
  topology,
  async tls(h) {
    // The member must fetch/pin the authority key and forward over verified TLS.
    // Unlike curl probes, this executes the production ureq construction.
    const response = await accountCall(h, '/api/collab/email/send_code',
      { email: 'tls-probe@example.invalid', purpose: 'login' }, undefined, 'member-a');
    check(response.status === 200, `Delegated TLS forwarding failed: ${JSON.stringify(response.value)}`);
    return { trust: 'authority.extraCaPem', transport: 'production ureq over private-CA HTTPS', forwarded: true };
  },
  async login() { delegatedBlocked(); },
  async relay() { delegatedBlocked(); },
  async access() { delegatedBlocked(); },
  async notifications(h) {
    const { old, fresh } = await pair(h);
    const r = await accountCall(h, '/api/interconnect/security/inbox', undefined, old.token);
    check(r.status === 200 && r.value.notifications.some(n => n.nodeId === fresh.nodeId || n.name === 'New device'), 'Old device did not receive enrollment notification');
    return { scope: 'authority HTTP session devices', notified: true };
  },
  async approvals(h) {
    const { old, fresh } = await pair(h);
    const operation = { nodeId: 'drill-risk-target' };
    const pending = await accountCall(h, '/api/peer/revoke', operation, fresh.token);
    check(pending.status === 403 && pending.value.error?.startsWith('approval_pending:'), 'High risk did not require approval');
    const requestId = pending.value.error.split(':')[1];
    const self = await accountCall(h, '/api/interconnect/security/decide', { requestId, approve: true }, fresh.token);
    check(self.status >= 400, 'Self approval was accepted');
    const approved = await accountCall(h, '/api/interconnect/security/decide', { requestId, approve: true }, old.token);
    check(approved.status === 200, 'Older device approval failed');
    const applied = await accountCall(h, '/api/peer/revoke', operation, fresh.token);
    check(applied.status === 200, 'Approved high risk operation failed');
    return { scope: 'authority HTTP session devices', approvalRequired: true, selfApprovalDenied: true, olderApprovalApplied: true };
  },
  async revoke(h) {
    const { old, fresh } = await pair(h);
    const started = performance.now();
    const result = await accountCall(h, '/api/interconnect/devices/revoke', { nodeId: fresh.nodeId }, old.token);
    check(result.status === 200, 'One-click revoke failed');
    await waitFor(async () => {
      const r = await accountCall(h, '/api/collab/me', undefined, fresh.token);
      return [401, 403].includes(r.status);
    }, { timeoutMs: h.pollSeconds * 1000, label: 'revoked session rejected' });
    return { scope: 'authority local invalidation; cross-host propagation remains blocked', revokeToDeniedMs: Math.round(performance.now() - started) };
  },
  async offline() { delegatedBlocked(); },
  async revocations() {
    throw new Blocked('Tamper/rollback delivery needs delegated TLS trust plus controlled signed-list replay endpoint. No replay/injection interface exists; do not substitute a unit test or claim that authority-side signing proves member rejection');
  },
  async r11(h) {
    const results = []; let pairIndex = 0;
    for (const [path, purpose] of [
      ['/api/collab/email/send_code', 'signup'], ['/api/collab/email/send_code', 'reset'],
      ['/api/collab/email/send_code', 'login'], ['/api/account/send_code', 'login'],
    ]) {
      const responses = [];
      for (const email of [`registered-${pairIndex}@example.invalid`, `unregistered-${pairIndex}@example.invalid`]) {
        responses.push(await accountCall(h, path, { email, purpose }));
      }
      check(responses.every(r => r.status === 200), 'R11 requires successful acceptance, not two common errors');
      check(JSON.stringify(responses[0]) === JSON.stringify(responses[1]), 'Registered/unregistered responses differ');
      results.push({ path, purpose, status: responses[0].status, equal: true }); pairIndex++;
    }
    return { comparisons: results, smtp: 'loopback port 1; asynchronous delivery intentionally fails; no external email' };
  },
  async rate(h) {
    let limited = 0;
    for (let i = 0; i <= h.settings.interconnect.loginRequests; i++) {
      const r = await accountCall(h, '/api/interconnect/login', { username: `missing-${i}`, password: 'invalid' });
      check([401, 429].includes(r.status), 'Unexpected rate-limit status');
      if (r.status === 429) { limited++; break; }
    }
    check(limited === 1, 'No HTTP 429 within configured login budget');
    return { rateLimitStatus: 429, configuredLoginRequests: h.settings.interconnect.loginRequests };
  },
  async backup(h) {
    const key = join(h.secrets, 'backup.agekey');
    await command(h, 'age-keygen', 'age-keygen', ['-o', key]);
    const recipient = (await command(h, 'age-recipient', 'age-keygen', ['-y', key])).text.trim();
    check(/^age1[a-z0-9]+$/.test(recipient), 'age recipient missing');
    const archive = join(h.secrets, 'backup.tar.age');
    const data = join(h.data.authority, 'Polaris/data');
    await compose(h, 'backup-stop', ['stop', '-t', '30', 'authority']);
    try {
      await fixture(h, 'backup', data, archive, recipient);
      check((await readFile(archive)).subarray(0, 22).toString().startsWith('age-encryption.org/v1'), 'Backup is not age encrypted');
      await fixture(h, 'restore', archive, join(h.secrets, 'restore-work'), key, data);
    } catch (error) { throw error; } // Remains stopped on verification failure; final cleanup removes it.
    await compose(h, 'restore-start', ['start', 'authority']);
    const pubkey = await waitFor(async () => {
      try { const r = await accountCall(h, '/api/account/pubkey'); return r.status === 200 ? r.value : false; } catch { return false; }
    }, { timeoutMs: h.timeout, label: 'restored authority' });
    check(pubkey.fingerprint === h.fingerprint, 'Restored authority fingerprint mismatch');
    const r = await accountCall(h, '/api/account/login', { username: 'drill-user', password: h.password });
    check(r.status === 200, 'Restored database cannot authenticate original account');
    return { encrypted: 'age', restoredFingerprint: h.fingerprint, accountRestored: true, snapshotImplementation: 'deploy/singapore/scripts/snapshot.py' };
  },
};
await run('D4', steps, async (h, name) => {
  if (!['login', 'relay', 'access', 'offline', 'revocations'].includes(name)) await topology(h);
});

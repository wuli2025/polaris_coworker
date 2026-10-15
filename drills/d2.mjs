import { join } from 'node:path';
import { randomBytes } from 'node:crypto';
import { respondDuring } from './lib/permissions.mjs';
import { run, check, waitFor, json, childEnv } from './lib/harness.mjs';
import { server, conversation, mapping } from './lib/agent.mjs';
import { start, ownedAgent } from './lib/process.mjs';
const steps = {
  async recovery(h) {
    const c = await conversation(h, 'crash');
    const marker = randomBytes(16).toString('hex');
    await h.api.turn(c.id, `Remember ${marker}. Say OK.`, { permissionMode: 'deny' });
    const before = await mapping(h, c.id);
    const t = await h.api.begin(c.id, 'Write a very long numbered list of 1000 distinct sentences about testing.', { permissionMode: 'deny' });
    await h.api.event(t.req, e => e.kind === 'delta');
    check(!h.api.forReq(t.req).some(e => e.kind === 'done'), 'Turn already ended before crash injection');
    const status = await h.api.invoke('agent_backend_status');
    await ownedAgent(h, status.pid);
    process.kill(status.pid, 'SIGKILL');
    const r = await h.api.end(t, true);
    const kinds = r.events.map(e => e.kind);
    check(kinds.indexOf('error') >= 0 && kinds.indexOf('error') < kinds.indexOf('done'), 'Expected error before done');
    const restarted = await waitFor(async () => {
      const s = await h.api.invoke('agent_backend_status');
      return s.healthy && s.pid !== status.pid && s.restarts > status.restarts ? s : false;
    }, { timeoutMs: h.timeout, label: 'supervisor restart' });
    const recovered = await h.api.turn(c.id, 'What marker did I ask you to remember?', { permissionMode: 'deny' });
    check(recovered.text.includes(marker), 'Recovered conversation lost history');
    check((await mapping(h, c.id)).id === before.id, 'Daemon session id changed after restart');
    return { oldPid: status.pid, newPid: restarted.pid, sessionId: before.id, eventKinds: kinds, ...recovered.metrics };
  },
  async circuit(h) {
    const killed = [];
    for (let n = 0; n < h.settings.host.failureThreshold; n++) {
      const s = await waitFor(async () => {
        const s = await h.api.invoke('agent_backend_status');
        return s.circuitOpen || (s.healthy && !killed.includes(s.pid)) ? s : false;
      }, { timeoutMs: h.timeout, label: 'healthy restart or circuit' });
      if (s.circuitOpen) break;
      await ownedAgent(h, s.pid); killed.push(s.pid); process.kill(s.pid, 'SIGKILL');
    }
    const s = await waitFor(async () => {
      const s = await h.api.invoke('agent_backend_status'); return s.circuitOpen ? s : false;
    }, { timeoutMs: h.timeout, label: 'open circuit' });
    check(!s.healthy && s.pid === null && /CircuitOpen/.test(s.lastError ?? ''), 'Inconsistent circuit diagnostics');
    return { crashes: killed.length, circuitOpen: s.circuitOpen, healthy: s.healthy, restarts: s.restarts };
  },
  async lock(h) {
    const file = join(h.secrets, 'lock-store.json');
    const helper = new URL('./lib/lock-child.mjs', import.meta.url).pathname;
    const held = start(h, 'lock-holder', process.execPath, ['--import', join(new URL('../', import.meta.url).pathname, 'agentd/node_modules/tsx/dist/loader.mjs'), helper, 'hold', file], { env: childEnv(h) });
    await waitFor(() => held.output.join('').includes('HELD\n'), { label: 'credential lock acquired' });
    const writer = start(h, 'lock-writer', process.execPath, ['--import', join(new URL('../', import.meta.url).pathname, 'agentd/node_modules/tsx/dist/loader.mjs'), helper, 'write', file], { env: childEnv(h) });
    await waitFor(() => writer.output.join('').includes('CONTENDING\n'), { label: 'second lock contender' });
    check(writer.child.exitCode === null, 'Second process exited before owner death');
    held.child.kill('SIGKILL'); await held.exited;
    const result = await writer.done();
    check(result.code === 0 && result.text.includes('WRITTEN'), 'Credential lock not reclaimed');
    const root = await json(file);
    check(root.credentials.p.key === 'drill-recovered', 'Credential transaction did not persist');
    return { holderKilled: true, contenderCompleted: true, persisted: true };
  },
};
await run('D2', steps, async (h, name) => { if (name !== 'lock') await server(h); });

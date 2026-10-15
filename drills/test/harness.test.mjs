const { test } = await import(process.env.VITEST ? 'vitest' : 'node:test');
import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { PassThrough } from 'node:stream';
import { mkdtemp, mkdir, writeFile, readFile, rm, access } from 'node:fs/promises';
import { join } from 'node:path';
import { options, integer, waitFor, Harness, Blocked, childEnv } from '../lib/harness.mjs';
import { descendants, signalTree } from '../lib/process-tree.mjs';
import { start } from '../lib/process.mjs';
import { Api } from '../lib/api.mjs';
import { assertSandbox } from '../lib/docker.mjs';
import { validateConfig, warnings, summary, lineCheck } from '../lib/regression.mjs';
import { validateSettings } from '../lib/settings.mjs';
import { respondDuring } from '../lib/permissions.mjs';
const config = JSON.parse(await readFile(new URL('../config/regression.json', import.meta.url), 'utf8'));
const temp = async fn => {
  const root = await mkdtemp('/tmp/polaris-drill-test-');
  try { await fn(root); } finally { await rm(root, { recursive: true, force: true }); }
};
test('CLI rejects unknown/missing steps, unknown flags and invalid repeat budgets', () => {
  assert.deepEqual(options(['--only', 'abort', '--keep', '--repeat', '3'], ['abort']), { only: 'abort', keep: true, repeat: 3 });
  for (const args of [['--only'], ['--only', 'typo'], ['--repeat', '0'], ['--repeat', '1.5'], ['--repeat'], ['--bad']]) {
    assert.throws(() => options(args, ['abort']), Blocked);
  }
  assert.equal(options([], []).repeat, 5);
});
test('typed integer configuration rejects blank, fractional and unbounded values', () => {
  assert.equal(integer({}, 'X', 8), 8);
  for (const v of ['', 'bad', '-1', '1.2', 'Infinity']) assert.throws(() => integer({ X: v }, 'X', 8), Blocked);
});
test('readiness uses a predicate and times out instead of accepting elapsed time', async () => {
  let probes = 0;
  assert.equal(await waitFor(() => ++probes === 3 && 'ready', { timeoutMs: 100, intervalMs: 1 }), 'ready');
  await assert.rejects(waitFor(() => false, { timeoutMs: 2, intervalMs: 1 }), /Timed out/);
  await assert.rejects(waitFor(() => { throw new Error('dead'); }), /dead/);
});
test('child environment never inherits product state or model keys', () => {
  const env = childEnv({ home: '/tmp/test-home', secrets: '/tmp/test-secret' });
  assert.equal(env.HOME, '/tmp/test-home');
  assert.equal(env.POLARIS_COLLAB_DB, undefined);
  assert.equal(env.OPENAI_API_KEY, undefined);
  assert.equal(env.NODE_OPTIONS, undefined);
});
function fakeHarness() {
  const h = new Harness();
  Object.assign(h, { home: '/tmp/fake-home', secrets: '/tmp/fake-secret', secretValues: new Set(['secret-value']),
    cleanup: [], commandTimeout: 100, logsCaptured: [] });
  h.log = async (name, text) => { h.logsCaptured.push([name, text]); };
  return h;
}
function fakeProcess(action) {
  return () => {
    const child = new EventEmitter(); child.stdout = new PassThrough(); child.stderr = new PassThrough();
    queueMicrotask(() => action(child));
    return child;
  };
}
test('fake command preserves nonzero status and redacts a secret split across pipe chunks', async () => {
  const h = fakeHarness();
  const p = start(h, 'fake', 'fake-command', [], {}, fakeProcess(child => {
    child.stdout.write('token secret-'); child.stdout.end('value\n');
    child.stderr.end('warning: example\n');
    setImmediate(() => child.emit('close', 37, null));
  }));
  const result = await p.done();
  assert.equal(result.code, 37);
  assert.match(result.text, /\[REDACTED\]/);
  assert.doesNotMatch(JSON.stringify(h.logsCaptured), /secret-value/);
  await p.stop();
});
test('missing executable produces blocked, not pass or a silent skip', async () => {
  const h = fakeHarness();
  const p = start(h, 'missing', 'absent', [], {}, fakeProcess(child => {
    child.emit('error', Object.assign(new Error('spawn absent'), { code: 'ENOENT' }));
    child.stdout.end(); child.stderr.end(); child.emit('close', -2, null);
  }));
  await assert.rejects(p.done(), Blocked);
});
test('request IDs isolate events; error→done is not a successful model turn', async () => {
  const h = fakeHarness(); h.timeout = 50;
  const api = new Api(h, 'http://127.0.0.1', 'secret-value'); api.ws = { readyState: 1 };
  api.events = [{ reqId: 'other', kind: 'delta', text: 'unrelated', at: 0 },
    { reqId: 'own', kind: 'error', at: 2 }, { reqId: 'own', kind: 'done', at: 3 }];
  await assert.rejects(api.end({ req: 'own', at: 1 }), /reported error/);
  const r = await api.end({ req: 'own', at: 1 }, true);
  assert.equal(r.text, ''); assert.deepEqual(r.metrics.eventKinds, ['error', 'done']);
  assert.equal(r.metrics.firstTokenMs, null); assert.equal(r.metrics.turnMs, 2);
});
test('permission responses are scoped to observed call/session and exclude another request', async () => {
  const calls = []; const h = fakeHarness(); h.timeout = 100;
  const api = new Api(h, '', '');
  api.events = [{ reqId: 'other', kind: 'permission.request', text: '{}' },
    { reqId: 'own', kind: 'permission.request', text: JSON.stringify({ sessionId: 's', callId: 'c', tool: 'bash' }) }];
  api.invoke = async (cmd, args) => { calls.push({ cmd, args }); api.events.push({ reqId: 'own', kind: 'done' }); };
  const t = respondDuring(api, { req: 'own' }, p => p.tool === 'bash' ? 'allow' : 'deny');
  await t.approvals;
  assert.equal(t.approvalError, undefined); assert.equal(calls.length, 1);
  assert.deepEqual(calls[0].args.response, { sessionId: 's', callId: 'c', decision: 'allow' });
});
function sandbox() {
  return { Mounts: [{ Type: 'bind', Source: '/tmp/project', Destination: '/workspace', RW: true },
    { Type: 'bind', Source: '/tmp/kb', Destination: '/kb', RW: false }],
    HostConfig: { CapDrop: ['ALL'], SecurityOpt: ['no-new-privileges:true'], Privileged: false,
      Memory: 4294967296, NanoCpus: 2000000000 } };
}
test('inspect rejects extra mounts, writable KB, capabilities and host namespaces', () => {
  assert.doesNotThrow(() => assertSandbox(sandbox(), '/tmp/project', '/tmp/kb'));
  for (const mutate of [v => v.Mounts.push({}), v => v.Mounts[1].RW = true,
    v => v.HostConfig.CapDrop = [], v => v.HostConfig.SecurityOpt = [],
    v => v.HostConfig.CapAdd = ['SYS_ADMIN'], v => v.HostConfig.PidMode = 'host']) {
    const v = sandbox(); mutate(v); assert.throws(() => assertSandbox(v, '/tmp/project', '/tmp/kb'));
  }
});
test('regression manifest validates argv and root traversal; counts warnings without Rust summaries', () => {
  assert.equal(validateConfig(structuredClone(config)).builds.length, 3);
  for (const mutate of [v => v.suites[0].cwd = '../outside', v => v.builds.pop(),
    v => v.maxLines = 501, v => v.suites[0].command = 'echo ok']) {
    const v = structuredClone(config); mutate(v); assert.throws(() => validateConfig(v), Blocked);
  }
  assert.equal(warnings('warning: unused import\nwarning: `crate` generated 1 warning\n(!) Large chunk\n'), 2);
  assert.deepEqual(summary([{ status: 'pass' }, { status: 'fail' }, { status: 'blocked' }]),
    { attempts: 3, pass: 1, fail: 1, blocked: 1, passRate: 1 / 3 });
});
test('line check counts physical blank lines and Unicode codepoints', async () => temp(async root => {
  await mkdir(join(root, 'owned'));
  await writeFile(join(root, 'owned', 'long.mjs'), '\n'.repeat(501));
  await writeFile(join(root, 'owned', 'wide.md'), '中'.repeat(501));
  const c = { ...config, lineRoots: ['owned'] };
  const result = await lineCheck(c, root);
  assert.equal(result.files, 2); assert.equal(result.violations.length, 2);
  assert.equal(result.violations.find(v => v.file.endsWith('wide.md')).columns, 501);
}));
test('--keep keeps diagnostics but removes credentials; cleanup is reverse order and idempotent', async () => temp(async root => {
  const h = fakeHarness(); const order = [];
  Object.assign(h, { root, home: join(root, 'home'), secrets: join(root, 'secrets'),
    logs: join(root, 'logs'), rows: [{ status: 'fail' }], options: { keep: true } });
  for (const path of [h.home, h.secrets, h.logs]) await mkdir(path);
  await writeFile(join(h.home, 'auth.json'), 'secret'); await writeFile(join(h.logs, 'public.log'), 'diagnostic');
  h.emit = async (step, status, evidence) => { h.rows.push({ step, status, evidence }); };
  h.cleanup = [async () => order.push(1), async () => order.push(2)];
  assert.equal(await h.finish(), 1); assert.equal(await h.finish(), 1);
  assert.deepEqual(order, [2, 1]);
  await assert.rejects(access(h.home)); await assert.rejects(access(h.secrets));
  assert.equal(await readFile(join(h.logs, 'public.log'), 'utf8'), 'diagnostic');
}));
test('nested step log names remain within the log directory', async () => temp(async root => {
  const h = new Harness(); h.logs = root; h.secretValues = new Set();
  await h.log('suites/1/agentd', 'safe');
  assert.equal(await readFile(join(root, 'suites-1-agentd.log'), 'utf8'), 'safe');
}));

test('runtime settings reject unknown keys, invalid ranges and floating image tags', async () => {
  const value = JSON.parse(await readFile(new URL('../config/runtime.json', import.meta.url), 'utf8'));
  assert.equal(validateSettings(value).host.failureThreshold, 4);
  for (const mutate of [v => v.host.failureThreshold = 0, v => v.host.restartMaxMs = 1,
    v => v.interconnect.caddyImage = 'caddy:latest', v => v.sandbox.unknown = true]) {
    const v = structuredClone(value); mutate(v); assert.throws(() => validateSettings(v), Blocked);
  }
});

test('cleanup targets only descendants and checks process identity before signalling', async () => {
  const records = new Map([[20, { parent: 10, started: 'a' }], [30, { parent: 20, started: 'b' }],
    [40, { parent: 99, started: 'c' }]]);
  assert.deepEqual(descendants(10, records).map(p => p.pid), [30, 20]);
  const killed = [];
  const stat = start => `30 (command with spaces) S 1 ${Array(17).fill('0').join(' ')} ${start}`;
  await signalTree([{ pid: 30, started: 'b' }, { pid: 40, started: 'old' }], 'SIGTERM',
    async path => stat(path.includes('/30/') ? 'b' : 'reused'), (pid, signal) => killed.push([pid, signal]));
  assert.deepEqual(killed, [[30, 'SIGTERM']]);
});

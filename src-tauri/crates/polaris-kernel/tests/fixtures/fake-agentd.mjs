// Protocol-only fixture. Never launches a child process or calls a model provider.
import { createInterface } from 'node:readline';
import { once } from 'node:events';
import { openSync, writeFileSync, fsyncSync, closeSync, existsSync } from 'node:fs';
const recordIndex = process.argv.indexOf('--record');
const record = recordIndex < 0 ? undefined : process.argv[recordIndex + 1];
if (process.argv.includes('--crash-start')) process.exit(7);
const sessions = new Map();
let applied;
let experts = { enabled: false, prompt: "deployment-prompt" };
async function gate(option) {
  const index = process.argv.indexOf(option);
  if (index < 0) return;
  while (!existsSync(process.argv[index + 1])) await new Promise(resolve => setTimeout(resolve, 5));
}
let nextSession = 0;
let nextTurn = 0;
let tail = Promise.resolve();
function send(value) {
  const line = JSON.stringify(value) + '\n';
  const task = tail.then(async () => {
    if (!process.stdout.write(line)) await once(process.stdout, 'drain');
  });
  tail = task.catch(() => {});
  return task;
}
const response = (id, result) => send({ type: 'response', id, ok: true, result });
const error = (id, code) => send({ type: 'response', id, ok: false, error: { code, message: code } });
const event = (s, name, fields = {}) => send({ type: 'event', sessionId: s.id, turnId: s.turn, event: name, ...fields });
async function turn(s, text) {
  await event(s, 'turn.started');
  if (text === 'permission') return event(s, 'permission.request', {
    targetSessionId: s.id, callId: 'approve-call', tool: 'bash', summary: 'pwd', expiresAt: Date.now() + 60000, parentCallId: 'parent-call'
  });
  if (text === 'crash') return process.exit(9);
  if (text === 'malformed') return process.stdout.write('not-json\n');
  if (text === 'oversized') return process.stdout.write('x'.repeat(65536) + '\n');
  if (text === 'hang' || text === 'abort-hang') return;
  if (text === 'failure') {
    await event(s, 'error', { code: 'MODEL_ERROR', message: 'fixture failure' });
    return event(s, 'turn.completed', { status: 'error' });
  }
  const count = text === 'flood' ? 1000 : 2;
  for (let i = 0; i < count && !s.aborted; i++) {
    await event(s, 'text.delta', { text: text === 'flood' ? 'x'.repeat(128) : ['hello ', 'world'][i] });
    if (text === 'flood') await event(s, 'tool.started', { callId: `c-${i}`, tool: 'read', args: { path: 'x' } });
  }
  if (!s.aborted) await event(s, 'turn.completed', { status: 'completed' });
}
async function dispatch({ id, method, params: p }) {
  if (record) {
    const fd = openSync(record, 'a');
    try {
      writeFileSync(fd, JSON.stringify({id,method,params:p}) + '\n');
      fsyncSync(fd);
    } finally { closeSync(fd); }
  }
  switch (method) {
    case 'permission.respond': {
      const s = sessions.get(p.sessionId);
      if (!s || p.callId !== 'approve-call') return error(id, 'APPROVAL_NOT_FOUND');
      await response(id, {accepted:true});
      await event(s, 'permission.resolved', {targetSessionId:s.id, callId:p.callId, reason:'responded'});
      await event(s, 'text.delta', {text:p.decision});
      return event(s, 'turn.completed', {status:'completed'});
    }
    case 'ping':
      if (process.argv.includes('--ping-hang')) return;
      return response(id, { status: 'ok', sessions: sessions.size, version: '0.1.0', piVersion: '1.0.4',
        usagePersistFailures: 7, experts, protocolVersion: process.argv.includes('--protocol-mismatch') ? 1 : 6 });
    case 'models.catalog':
      await gate('--gate-catalog');
      return response(id, { models: [] });
    case 'models.list': return response(id, { models: applied ? [applied.defaultModel] : [{ provider: 'fixture', id: 'test' }] });
    case 'session.open': {
      if (p.sessionId && sessions.has(p.sessionId)) return error(id, 'SESSION_ALREADY_OPEN');
      const sid = p.sessionId ?? `fake-${++nextSession}`;
      sessions.set(sid, { id: sid });
      return response(id, { sessionId: sid, resumed: !!p.sessionId, messageCount: 0,
        model: { provider: 'fixture', id: 'test' }, thinkingLevel: 'off', tools: ['read'], reopenRequired: false });
    }
    case 'session.configure': {
      if (!sessions.has(p.sessionId)) return error(id, 'SESSION_NOT_FOUND');
      return response(id, {sessionId:p.sessionId, model:p.model ?? {provider:'fixture',id:'test'}, tools:['read']});
    }
    case 'prompt': {
      const s = sessions.get(p.sessionId);
      if (!s) return error(id, 'SESSION_NOT_FOUND');
      s.turn = `turn-${++nextTurn}`;
      s.text = p.text;
      s.aborted = false;
      // Exercise events racing ahead of the prompt response, as real agentd does.
      void turn(s, p.text);
      return response(id, { turnId: s.turn, disposition: 'started' });
    }
    case 'abort': {
      const s = sessions.get(p.sessionId);
      if (!s) return error(id, 'SESSION_NOT_FOUND');
      if (s.text === 'abort-hang') return;
      s.aborted = true;
      await event(s, 'turn.completed', { status: 'aborted' });
      return response(id, { aborted: true });
    }
    case 'close': sessions.delete(p.sessionId); return response(id, { closed: true });
    case 'config.validate':
      if (p.config?.providers?.[0]?.models?.[0]?.id === 'invalid-wire-config') {
        return send({type:'response', id, ok:false, error:{code:'INVALID_CONFIG', message:'配置字段校验失败。',
          paths:['/providers/0/models/0/name']}});
      }
      return response(id, { valid: true });
    case 'config.apply':
      await gate('--gate-apply');
      applied = p.config;
      if (p.config?.experts) experts = p.config.experts;
      return response(id, { applied: true, reopenRequired: [...sessions.keys()] });
    case 'steer': case 'followUp': return response(id, { turnId: sessions.get(p.sessionId)?.turn, disposition: 'queued' });
    default: return error(id, 'INVALID_REQUEST');
  }
}
const lines = createInterface({ input: process.stdin });
for await (const line of lines) {
  try { void dispatch(JSON.parse(line)); }
  catch { await error(null, 'INVALID_JSON'); }
}
await tail;

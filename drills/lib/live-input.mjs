import { randomBytes } from 'node:crypto';
import { check, waitFor } from './harness.mjs';
import { request } from './api.mjs';
import { conversation, mapping, sessionEntries, memory } from './agent.mjs';

// Hold an actual tool approval open as a readiness barrier; never race model latency with a sleep.
export async function liveInput(h, mode) {
  const c = await conversation(h, mode);
  const first = `FIRST_${randomBytes(8).toString('hex')}`;
  const next = `NEXT_${randomBytes(8).toString('hex')}`;
  const turn = await h.api.begin(c.id,
    `Use bash to run printf '${first}'. After it completes, answer exactly ${first}.`, { permissionMode: 'manual' });
  try {
    const event = await h.api.event(turn.req, e => e.kind === 'permission.request');
    const permission = JSON.parse(event.text);
    check(permission.tool === 'bash', 'Expected bash permission barrier');
    const before = await mapping(h, c.id);
    const cmd = mode === 'steer' ? 'chat_steer' : 'chat_follow_up';
    const text = mode === 'steer'
      ? `Change the final answer to exactly ${next}.`
      : `After completing the current task, answer exactly ${next}.`;
    const receipt = await waitFor(async () => {
      const r = await request(h.api.base, '/api/invoke', {
        cmd, args: { reqId: turn.req, conversationId: c.id, text },
      }, h.api.token);
      if (r.status === 409 && ['ChatTurnNotReady', 'TURN_NOT_READY'].some(code => r.value.error?.startsWith(code))) return false;
      check(r.status === 200 && r.value.disposition === 'queued', 'Running input was not queued');
      return r.value;
    }, { timeoutMs: h.timeout, label: 'running input accepted' });
    check(typeof receipt.turnId === 'string', 'Missing correlated agentd turn');
    await h.api.invoke('chat_permission_respond', { response: {
      sessionId: permission.sessionId, callId: permission.callId, decision: 'allow',
    } });
    const result = await h.api.end(turn);
    check(result.text.includes(next), 'Queued input did not affect the original request output');
    const after = await mapping(h, c.id);
    check(before.id === after.id, 'Running input changed agentd session');
    const messages = (await sessionEntries(h, c.id)).filter(e => e.type === 'message').map(e => e.message);
    const queuedAt = messages.findIndex(m => m.role === 'user' && JSON.stringify(m.content).includes(next));
    check(queuedAt >= 0, 'Queued input absent from persisted session');
    check(messages.slice(queuedAt + 1).some(m => m.role === 'assistant' && JSON.stringify(m.content).includes(next)),
      'No persisted assistant response after queued user input');
    if (mode === 'followUp') {
      check(messages.slice(0, queuedAt).some(m => m.role === 'assistant' && !m.content.some(p => p.type === 'toolCall')
        && JSON.stringify(m.content).includes(first)), 'Follow-up ran before original answer finished');
    }
    const idle = await request(h.api.base, '/api/invoke', { cmd, args: { reqId: turn.req, text: 'too late' } }, h.api.token);
    check(idle.status === 409 && idle.value.error === 'ChatNotRunning', 'Idle input was accepted');
    return { ...result.metrics, ...await memory(h), sameSession: true, idleRejected: true, turnId: receipt.turnId };
  } catch (error) {
    await h.api.invoke('chat_cancel', { reqId: turn.req }).catch(() => {});
    throw error;
  }
}

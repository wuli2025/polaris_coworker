import { check, waitFor } from './harness.mjs';
export async function request(base, path, body, token, timeoutMs = 10000) {
  const response = await fetch(`${base}${path}`, { method: body === undefined ? 'GET' : 'POST',
    headers: { 'Content-Type': 'application/json', ...(token ? { Authorization: `Bearer ${token}` } : {}) },
    body: body === undefined ? undefined : JSON.stringify(body), signal: AbortSignal.timeout(timeoutMs) });
  const text = await response.text();
  let value; try { value = JSON.parse(text); } catch { value = text; }
  return { status: response.status, value };
}
export class Api {
  constructor(h, base, token) { this.h = h; this.base = base; this.token = token; this.events = []; }
  async invoke(cmd, args = {}) {
    const r = await request(this.base, '/api/invoke', { cmd, args }, this.token);
    if (r.status !== 200) {
      // Keep the server's error text for diagnosis; h.log redacts known secrets.
      const error = String(r.value?.error ?? r.value ?? '').slice(0, 300);
      await this.h.log('api', JSON.stringify({ cmd, status: r.status, error }) + '\n');
      throw new Error(`invoke ${cmd} status ${r.status}`);
    }
    return r.value;
  }
  async connect() {
    const ws = new WebSocket(`${this.base.replace('http:', 'ws:')}/ws?token=${encodeURIComponent(this.token)}`);
    let error = false;
    ws.addEventListener('error', () => { error = true; });
    ws.addEventListener('message', message => {
      try {
        const ev = JSON.parse(message.data);
        if (ev.topic === 'chat:stream') this.events.push({ ...ev.payload, at: performance.now() });
      } catch { error = true; }
    });
    this.h.cleanup.push(() => ws.close());
    await waitFor(() => { check(!error, 'WebSocket failed'); return ws.readyState === 1; }, { label: 'WebSocket open' });
    this.ws = ws;
  }
  forReq(req) { return this.events.filter(e => e.reqId === req); }
  async event(req, predicate, timeoutMs = this.h.timeout) {
    return waitFor(() => {
      check(this.ws.readyState === 1, 'WebSocket closed');
      return this.forReq(req).find(predicate);
    }, { timeoutMs, label: 'correlated chat event' });
  }
  async begin(conversationId, prompt, extras = {}) {
    const at = performance.now();
    const req = await this.invoke('chat_send', { args: { prompt, conversationId,
      permissionMode: 'auto_current', workMode: 'work', ...extras } });
    check(typeof req === 'string', 'chat_send did not return request id');
    return { req, at };
  }
  async end(turn, allowErrors = false) {
    const done = await this.event(turn.req, e => e.kind === 'done');
    if (turn.approvals) await turn.approvals;
    if (turn.approvalError) throw turn.approvalError;
    const events = this.forReq(turn.req);
    check(events.filter(e => e.kind === 'done').length === 1, 'duplicate done');
    if (!allowErrors) check(!events.some(e => e.kind === 'error'), 'turn reported error');
    const first = events.find(e => e.kind === 'delta' && e.text);
    const metrics = { reqId: turn.req, firstTokenMs: first ? Math.round(first.at - turn.at) : null,
      turnMs: Math.round(done.at - turn.at), eventKinds: events.map(e => e.kind) };
    await this.h.log('events', JSON.stringify(metrics) + '\n');
    return { events, text: events.filter(e => e.kind === 'delta').map(e => e.text ?? '').join(''), metrics, done };
  }
  async turn(conv, prompt, extras = {}) { return this.end(await this.begin(conv, prompt, extras)); }
}

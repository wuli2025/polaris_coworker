import { check, waitFor } from './harness.mjs';
// Decisions are scoped to one reqId and one callId. Never approve unrelated sessions.
export function respondDuring(api, turn, decision) {
  const seen = new Set();
  turn.approvals = (async () => {
    while (true) {
      const event = await waitFor(() => api.forReq(turn.req).find(e =>
        e.kind === 'done' || (e.kind === 'permission.request' && !seen.has(e.text))),
      { timeoutMs: api.h.timeout, label: 'permission request or done' });
      if (event.kind === 'done') return;
      seen.add(event.text);
      const payload = JSON.parse(event.text);
      const response = decision(payload);
      if (!response) continue;
      try {
        await api.invoke('chat_permission_respond', { response: {
          sessionId: payload.sessionId, callId: payload.callId, decision: response,
        } });
      } catch (error) {
        // Cancelling a turn expires its pending approvals by design; a late answer is expected then.
        if (!(turn.cancelled && error.serverCode === 'APPROVAL_NOT_FOUND')) throw error;
      }
    }
  })().catch(error => { turn.approvalError = error; });
  return turn;
}
export async function approvedBash(api, conv, prompt) {
  const t = respondDuring(api, await api.begin(conv, prompt), p => p.tool === 'bash' ? 'allow' : 'deny');
  return api.end(t);
}

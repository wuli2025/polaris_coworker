export type Decision = "allow" | "deny" | "allowAlways";
export interface Approval {
  reqId: string;
  sessionId: string;
  callId: string;
  tool: string;
  summary: string;
  parentCallId?: string;
}
export interface PermissionEvent { kind: string; reqId: string; text?: string | null }
export function consume(queue: Approval[], event: PermissionEvent): Approval[] {
  if (event.kind === "done") return queue.filter((item) => item.reqId !== event.reqId);
  if (event.kind !== "permission.request" || !event.text) return queue;
  try {
    const value: unknown = JSON.parse(event.text);
    if (!value || typeof value !== "object") return queue;
    const approval = value as Record<string, unknown>;
    if (!["sessionId", "callId", "tool", "summary"].every((key) => typeof approval[key] === "string")) return queue;
    const item: Approval = { reqId: event.reqId, sessionId: String(approval.sessionId), callId: String(approval.callId),
      tool: String(approval.tool), summary: String(approval.summary),
      parentCallId: typeof approval.parentCallId === "string" ? approval.parentCallId : undefined };
    if (queue.some((entry) => entry.callId === item.callId && entry.sessionId === item.sessionId)) return queue;
    return [...queue, item];
  } catch { return queue; }
}

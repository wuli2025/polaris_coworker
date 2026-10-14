export type Decision = "allow" | "deny" | "allowAlways";
export interface Approval {
  reqId: string;
  sessionId: string;
  callId: string;
  tool: string;
  summary: string;
  expiresAt: number;
  parentCallId?: string;
}
export interface PermissionEvent { kind: string; reqId: string; text?: string | null }
export function consume(queue: Approval[], event: PermissionEvent): Approval[] {
  if (event.kind === "done") return queue.filter((item) => item.reqId !== event.reqId);
  if (!["permission.request", "permission.resolved"].includes(event.kind) || !event.text) return queue;
  try {
    const value: unknown = JSON.parse(event.text);
    if (!value || typeof value !== "object") return queue;
    const approval = value as Record<string, unknown>;
    if (event.kind === "permission.resolved") {
      return queue.filter((item) => item.sessionId !== approval.sessionId || item.callId !== approval.callId);
    }
    if (typeof approval.expiresAt !== "number" || !Number.isSafeInteger(approval.expiresAt) || approval.expiresAt < 0) return queue;
    if (!["sessionId", "callId", "tool", "summary"].every((key) => typeof approval[key] === "string")) return queue;
    const item: Approval = { reqId: event.reqId, sessionId: String(approval.sessionId), callId: String(approval.callId),
      tool: String(approval.tool), summary: String(approval.summary), expiresAt: approval.expiresAt,
      parentCallId: typeof approval.parentCallId === "string" ? approval.parentCallId : undefined };
    if (queue.some((entry) => entry.callId === item.callId && entry.sessionId === item.sessionId)) return queue;
    return [...queue, item];
  } catch { return queue; }
}

export function remainingSeconds(approval: Approval, now: number): number {
  return Math.max(0, Math.ceil((approval.expiresAt - now) / 1000));
}

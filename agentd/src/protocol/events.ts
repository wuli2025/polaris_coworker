import { Type, type Static, type TSchema } from "typebox";
const text = Type.String();
function event<N extends string, P extends Record<string, TSchema>>(name: N, properties: P) {
  return Type.Object({ type: Type.Literal("event"), event: Type.Literal(name),
    sessionId: text, turnId: text, ...properties });
}
const tokens = Type.Number({ minimum: 0 });
export const UsageSchema = Type.Object({
  input: tokens, output: tokens, cacheRead: tokens, cacheWrite: tokens, totalTokens: tokens,
  cost: Type.Object({ input: tokens, output: tokens, cacheRead: tokens, cacheWrite: tokens, total: tokens }),
});
export const EventSchema = Type.Union([
  event("turn.started", {}), event("text.delta", { text }), event("reasoning.delta", { text }),
  event("tool.started", { callId: text, tool: text, args: Type.Unknown(), parentCallId: Type.Optional(text) }),
  event("tool.completed", { callId: text, tool: text, result: Type.Unknown(),
    isError: Type.Boolean(), parentCallId: Type.Optional(text) }),
  event("usage", { usage: UsageSchema }),
  event("turn.completed", { status: Type.Union([Type.Literal("completed"), Type.Literal("aborted"), Type.Literal("error"), Type.Literal("timeout")]) }),
  event("error", { code: text, message: text }),
]);
export type ProtocolEvent = Static<typeof EventSchema>;
export type TurnStatus = Extract<ProtocolEvent, { event: "turn.completed" }>["status"];

import { describe, expect, it } from "vitest";
import { consume } from "./queue";
describe("permission confirmation queue", () => {
  const approval = { kind: "permission.request", reqId: "request", text: JSON.stringify({
    sessionId: "child-session", callId: "call", tool: "bash", summary: "pwd", parentCallId: "parent",
  }) };
  it("retains child identity, deduplicates and removes requests on terminal events", () => {
    const queue = consume([], approval);
    expect(queue[0].sessionId).toBe("child-session");
    expect(queue[0].parentCallId).toBe("parent");
    expect(consume(queue, approval)).toHaveLength(1);
    expect(consume(queue, { kind: "done", reqId: "unrelated" })).toHaveLength(1);
    expect(consume(queue, { kind: "done", reqId: "request" })).toEqual([]);
  });
  it("ignores malformed requests and unrelated chat events", () => {
    for (const text of ["invalid JSON", "null", "{}", '{"tool":5}']) {
      expect(consume([], { ...approval, text })).toEqual([]);
    }
    expect(consume([], { ...approval, kind: "delta" })).toEqual([]);
  });
});

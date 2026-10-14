import assert from "node:assert/strict";
import { test } from "node:test";
import { Value } from "typebox/value";
import { Type } from "typebox";
import { fauxAssistantMessage, fauxThinking, fauxText, fauxToolCall } from "@earendil-works/pi-ai";
import type { ExtensionFactory } from "@earendil-works/pi-coding-agent";
import { EventMapper } from "../src/events/mapper.ts";
import { EventSchema, type ProtocolEvent } from "../src/protocol/events.ts";
import { harness, result } from "./helpers.ts";

const echo: ExtensionFactory = (pi) => pi.registerTool({
  name: "echo", label: "Echo", description: "Echo text or fail", parameters: Type.Object({ text: Type.String() }),
  async execute(_id, args) {
    if (args.text === "fail") throw new Error("tool failed");
    return { content: [{ type: "text", text: args.text }], details: {} };
  },
});
test("actual SDK text/thinking, successful and failed tools, usage map to validated protocol", async () => {
  const message = fauxAssistantMessage([fauxThinking("considering"), fauxText("final answer")]);
  message.usage = { input: 10, output: 20, cacheRead: 3, cacheWrite: 4, totalTokens: 37,
    cost: { input: 1, output: 2, cacheRead: 3, cacheWrite: 4, total: 10 } };
  const h = await harness([
    fauxAssistantMessage([fauxToolCall("echo", { text: "hi" }), fauxToolCall("echo", { text: "fail" })], { stopReason: "toolUse" }),
    message,
  ], { tools: ["echo"], extensions: { enabled: true } }, [echo]);
  try {
    const id = result<{ sessionId: string }>(await h.request("session.open", { cwd: h.dir })).sessionId;
    await h.request("prompt", { sessionId: id, text: "call tools" });
    await h.daemon.sessions.wait(id);
    for (const event of h.events) assert.ok(Value.Check(EventSchema, event), JSON.stringify(event));
    assert.equal(h.events.filter((e) => e.event === "text.delta").map((e) => e.text).join(""), "final answer");
    assert.equal(h.events.filter((e) => e.event === "reasoning.delta").map((e) => e.text).join(""), "considering");
    const started = h.events.filter((e) => e.event === "tool.started");
    const completed = h.events.filter((e) => e.event === "tool.completed");
    assert.equal(started.length, 2);
    assert.deepEqual(completed.map((e) => e.isError).sort(), [false, true]);
    assert.deepEqual(started.map((e) => e.callId).sort(), completed.map((e) => e.callId).sort());
    assert.ok(h.events.filter((e) => e.event === "usage").at(-1)!.usage.totalTokens > 0);
    assert.equal(h.events.at(-1)?.event, "turn.completed");
  } finally { await h.close(); }
});
test("message_end fallback does not duplicate streamed text; low-level agent_end is not done", () => {
  const events: ProtocolEvent[] = [];
  const mapper = new EventMapper("s", "t", (event) => events.push(event));
  const message = fauxAssistantMessage("hello");
  mapper.accept({ type: "message_start", message });
  mapper.accept({ type: "message_update", message,
    assistantMessageEvent: { type: "text_delta", contentIndex: 0, delta: "hello", partial: message } });
  mapper.accept({ type: "message_end", message });
  mapper.accept({ type: "agent_end", messages: [message], willRetry: false });
  assert.deepEqual(events.filter((e) => e.event === "text.delta").map((e) => e.text), ["hello"]);
  assert.ok(!events.some((e) => e.event === "turn.completed"));
  mapper.accept({ type: "message_start", message });
  mapper.accept({ type: "message_end", message });
  assert.equal(events.filter((e) => e.event === "text.delta").length, 2);
});
test("model errors terminate with error; raw provider diagnostics never reach events", async () => {
  const h = await harness([fauxAssistantMessage("", { stopReason: "error", errorMessage: "Bearer dangerous-private-value" })]);
  try {
    const id = result<{ sessionId: string }>(await h.request("session.open", { cwd: h.dir })).sessionId;
    await h.request("prompt", { sessionId: id, text: "fail" });
    await h.daemon.sessions.wait(id);
    assert.ok(h.events.some((e) => e.event === "error" && e.code === "MODEL_ERROR"));
    assert.ok(h.events.some((e) => e.event === "turn.completed" && e.status === "error"));
    assert.ok(!JSON.stringify(h.events).includes("dangerous-private-value"));
  } finally { await h.close(); }
});

test("usage preserves exact token and cost fields from authoritative SDK message", () => {
  const events: ProtocolEvent[] = [];
  const mapper = new EventMapper("s", "t", (event) => events.push(event));
  const message = fauxAssistantMessage("done");
  message.usage = { input: 10, output: 20, cacheRead: 3, cacheWrite: 4, totalTokens: 37,
    cost: { input: 1, output: 2, cacheRead: 3, cacheWrite: 4, total: 10 } };
  mapper.accept({ type: "message_end", message });
  assert.deepEqual(events.find((e) => e.event === "usage")?.usage, message.usage);
});

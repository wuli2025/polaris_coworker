import assert from "node:assert/strict";
import { test } from "node:test";
import { fauxAssistantMessage, fauxToolCall } from "@earendil-works/pi-ai";
import { harness, result, until, type Steps } from "./helpers.ts";

const tool = (name: string, args: Parameters<typeof fauxToolCall>[1]) => fauxAssistantMessage(fauxToolCall(name, args), { stopReason: "toolUse" });
const slow: Steps[number] = (_ctx, options) => new Promise((resolve) => {
  const abort = () => resolve(fauxAssistantMessage("cancelled", { stopReason: "aborted" }));
  if (options?.signal?.aborted) abort(); else options?.signal?.addEventListener("abort", abort, { once: true });
});
test("children inherit deny despite visible write tools and keep completion in parent turn", async () => {
  const h = await harness([
    tool("subagent", { task: "child write" }), tool("write", { path: "forbidden", content: "x" }),
    fauxAssistantMessage("child summary"), fauxAssistantMessage("parent done"),
  ]);
  try {
    const { sessionId } = result<any>(await h.request("session.open", { cwd: h.dir, toolProfile: "full", permissionMode: "deny" }));
    await h.request("prompt", { sessionId, text: "delegate" });
    await h.daemon.sessions.wait(sessionId);
    assert.equal(h.events.filter((e) => e.event === "turn.completed").length, 1);
    assert.ok(h.events.some((e) => e.event === "tool.completed" && e.tool === "write" && e.isError && e.parentCallId));
    assert.ok(h.events.some((e) => e.event === "tool.completed" && e.tool === "subagent"
      && JSON.stringify(e.result).includes("child summary")));
    assert.equal(h.daemon.sessions.size, 1);
  } finally { await h.close(); }
});
test("subagent recursion is bounded", async () => {
  const h = await harness([
    tool("subagent", { task: "child" }), tool("subagent", { task: "grandchild" }),
    fauxAssistantMessage("child stopped"), fauxAssistantMessage("done"),
  ], { subagents: { maxDepth: 1, maxConcurrent: 1, summaryChars: 1000 } });
  try {
    const { sessionId } = result<any>(await h.request("session.open", { cwd: h.dir, toolProfile: "full" }));
    await h.request("prompt", { sessionId, text: "delegate" });
    await h.daemon.sessions.wait(sessionId);
    assert.ok(h.events.some((e) => e.event === "tool.completed" && e.isError
      && JSON.stringify(e.result).includes("Maximum subagent depth")));
    assert.equal(h.daemon.sessions.size, 1);
  } finally { await h.close(); }
});
test("concurrency cap and parent abort cascade to every child", async () => {
  const h = await harness([slow, slow], { subagents: { maxDepth: 2, maxConcurrent: 1, summaryChars: 1000 } });
  try {
    const { sessionId } = result<any>(await h.request("session.open", { cwd: h.dir, toolProfile: "full" }));
    await h.request("prompt", { sessionId, text: "wait" });
    await until(() => h.faux.state.callCount === 1);
    const child = h.daemon.sessions.subagent(sessionId, "parent-call", "child");
    const outcome = child.then(() => "completed", () => "aborted");
    await until(() => h.faux.state.callCount === 2);
    await assert.rejects(h.daemon.sessions.subagent(sessionId, "second", "second child"), /Maximum concurrent/);
    await h.request("abort", { sessionId });
    assert.equal(await outcome, "aborted");
    assert.equal(h.daemon.sessions.size, 1);
  } finally { await h.close(); }
});
test("sandbox setting is inherited and requests retain parentCallId", async () => {
  const h = await harness([tool("subagent", { task: "child" }), tool("bash", { command: "pwd" }),
    fauxAssistantMessage("child summary"), fauxAssistantMessage("done")]);
  try {
    const { sessionId } = result<any>(await h.request("session.open", { cwd: h.dir,
      toolProfile: "full", permissionMode: "auto-all", sandbox: true }));
    await h.request("prompt", { sessionId, text: "delegate" });
    const handled = new Set<string>();
    await until(() => {
      for (const event of h.events) if (event.event === "sandbox.request" && !handled.has(event.callId)) {
        handled.add(event.callId);
        assert.equal(event.sessionId, sessionId);
        if (event.operation === "bash") {
          assert.ok(event.parentCallId);
          assert.notEqual(event.targetSessionId, sessionId);
        }
        void h.request("sandbox.respond", { sessionId: event.targetSessionId, callId: event.callId,
          output: "/container", exitCode: 0, signal: null, timedOut: false });
      }
      return h.events.some((event) => event.event === "turn.completed");
    });
    await h.daemon.sessions.wait(sessionId);
    assert.equal(h.events.filter((e) => e.event === "turn.completed").length, 1);
  } finally { await h.close(); }
});

test("child permission expiry reaches parent stream with child identity before turn completion", async () => {
  const h = await harness([tool("subagent", { task: "child" }), tool("bash", { command: "pwd" }),
    fauxAssistantMessage("child summary"), fauxAssistantMessage("done")],
    { permissions: { defaultMode: "manual", timeoutMs: 20, summaryChars: 100 } });
  try {
    const { sessionId } = result<{ sessionId: string }>(await h.request("session.open", { cwd: h.dir, toolProfile: "full" }));
    await h.request("prompt", { sessionId, text: "delegate" });
    await h.daemon.sessions.wait(sessionId);
    const approval = h.events.find((event) => event.event === "permission.request");
    const resolved = h.events.find((event) => event.event === "permission.resolved");
    assert.ok(approval?.event === "permission.request" && resolved?.event === "permission.resolved");
    assert.equal(resolved.reason, "expired");
    assert.equal(resolved.sessionId, sessionId);
    assert.equal(resolved.targetSessionId, approval.targetSessionId);
    assert.notEqual(resolved.targetSessionId, sessionId);
    assert.equal(resolved.callId, approval.callId);
    assert.equal(resolved.parentCallId, approval.parentCallId);
    assert.ok(h.events.indexOf(resolved) < h.events.findIndex((event) => event.event === "turn.completed"));
  } finally { await h.close(); }
});

import assert from "node:assert/strict";
import { test } from "node:test";
import { setTimeout as delay } from "node:timers/promises";
import { fauxAssistantMessage, fauxToolCall } from "@earendil-works/pi-ai";
import { Sandbox } from "../src/extensions/sandbox.ts";
import { loadConfig, mergeConfig } from "../src/config/load.ts";
import { harness, result, until } from "./helpers.ts";

const config = { ...loadConfig().sandbox, probeTimeoutMs: 20, probeCacheMs: 30, timeoutMs: 2000 };
test("unresponsive backend fails fast, caches failure, and retries after TTL", async () => {
  let calls = 0;
  const sandbox = new Sandbox(true, "/tmp", config, () => { calls++; });
  const start = performance.now();
  await assert.rejects(sandbox.request("bash", { command: "pwd" }), /SANDBOX_UNAVAILABLE/);
  assert.ok(performance.now() - start < 250, "must fail near probe timeout, not 2000ms tool timeout");
  assert.equal(calls, 1);
  await assert.rejects(sandbox.available(), /SANDBOX_UNAVAILABLE/);
  assert.equal(calls, 1);
  await delay(config.probeCacheMs + 5);
  await assert.rejects(sandbox.available(), /SANDBOX_UNAVAILABLE/);
  assert.equal(calls, 2);
});
test("successful probe is shared and cached, config rejects invalid probe limits", async () => {
  let calls = 0;
  const sandbox = new Sandbox(true, "/tmp", config, (request) => {
    calls++;
    queueMicrotask(() => sandbox.respond(request.callId, { output: "", exitCode: 0, signal: null, timedOut: false }));
  });
  await Promise.all([sandbox.available(), sandbox.available()]);
  await sandbox.available();
  assert.equal(calls, 1);
  await delay(config.probeCacheMs + 5);
  await sandbox.available();
  assert.equal(calls, 2);
  for (const patch of [{ probeTimeoutMs: 0 }, { probeCacheMs: -1 }, { probeTimeoutMs: "5000" }]) {
    assert.throws(() => mergeConfig(loadConfig(), { sandbox: { ...config, ...patch } }));
  }
});
for (const status of [
  { exitCode: 7, signal: null, timedOut: false },
  { exitCode: 137, signal: 9, timedOut: false },
  { exitCode: 137, signal: 9, timedOut: true },
]) test(`Pi bash preserves failure ${JSON.stringify(status)}`, async () => {
  const h = await harness([fauxAssistantMessage([fauxToolCall("bash", { command: "false" }, { id: "failure" })]), fauxAssistantMessage("done")]);
  try {
    const { sessionId } = result<{ sessionId: string }>(await h.request("session.open", {
      cwd: h.dir, toolProfile: "full", permissionMode: "auto-all", sandbox: true,
    }));
    await h.request("prompt", { sessionId, text: "fail" });
    const handled = new Set<string>();
    await until(() => {
      for (const event of h.events) if (event.event === "sandbox.request" && !handled.has(event.callId)) {
        handled.add(event.callId);
        void h.request("sandbox.respond", { sessionId, callId: event.callId, output: "stderr retained",
          ...(event.operation === "probe" ? { exitCode: 0, signal: null, timedOut: false } : status) });
      }
      return h.events.some((event) => event.event === "turn.completed");
    });
    const event = h.events.find((event) => event.event === "tool.completed");
    assert.ok(event?.event === "tool.completed" && event.isError);
    assert.match(JSON.stringify(event.result), new RegExp(String(status.exitCode)));
    assert.match(JSON.stringify(event.result), /stderr retained/);
    if (status.timedOut) assert.match(JSON.stringify(event.result), /SANDBOX_TIMEOUT/);
    if (status.signal) assert.match(JSON.stringify(event.result), /SANDBOX_SIGNAL/);
  } finally { await h.close(); }
});
test("sandbox session probes before even a read-only tool and fails without approval", async () => {
  const h = await harness([fauxAssistantMessage([fauxToolCall("read", { path: "missing" }, { id: "read" })]), fauxAssistantMessage("done")],
    { sandbox: config });
  try {
    const { sessionId } = result<{ sessionId: string }>(await h.request("session.open", { cwd: h.dir, toolProfile: "readonly", sandbox: true }));
    await h.request("prompt", { sessionId, text: "read" });
    await h.daemon.sessions.wait(sessionId);
    assert.equal(h.events.filter((event) => event.event === "sandbox.request").length, 1);
    const completed = h.events.find((event) => event.event === "tool.completed");
    assert.ok(completed?.event === "tool.completed" && completed.isError);
    assert.match(JSON.stringify(completed), /SANDBOX_UNAVAILABLE/);
  } finally { await h.close(); }
});

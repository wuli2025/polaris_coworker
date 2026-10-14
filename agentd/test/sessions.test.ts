import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { test } from "node:test";
import { setTimeout as delay } from "node:timers/promises";
import { fauxAssistantMessage } from "@earendil-works/pi-ai";
import { harness, result, until, type Steps } from "./helpers.ts";

const slow: Steps[number] = (_context, options) => new Promise((resolve) => {
  const timer = setTimeout(() => resolve(fauxAssistantMessage("late")), 2000);
  options?.signal?.addEventListener("abort", () => {
    clearTimeout(timer);
    resolve(fauxAssistantMessage("", { stopReason: "aborted" }));
  }, { once: true });
});
async function open(h: Awaited<ReturnType<typeof harness>>) {
  return result<{ sessionId: string }>(await h.request("session.open", { cwd: h.dir })).sessionId;
}
test("open → prompt → close → fresh daemon resumes history by id", async () => {
  const h = await harness([fauxAssistantMessage("remember me")]);
  try {
    const id = await open(h);
    const turn = result<{ turnId: string }>(await h.request("prompt", { sessionId: id, text: "hello history" }));
    await h.daemon.sessions.wait(id);
    assert.deepEqual(h.events.filter((e) => e.event === "turn.completed").map((e) => e.status), ["completed"]);
    assert.ok(h.events.every((e) => e.sessionId === id && e.turnId === turn.turnId));
    await h.request("close", { sessionId: id });
    const file = readdirSync(h.daemon.config.sessionDir).find((p) => p.endsWith(".jsonl"))!;
    assert.match(readFileSync(join(h.daemon.config.sessionDir, file), "utf8"), /hello history/);
    // A new registry/daemon proves restoration is disk-backed, rather than a retained map.
    const { Daemon } = await import("../src/daemon.ts");
    const second = await Daemon.create(h.daemon.config, () => {}, () => {}, { credentials: h.daemon.models.credentials });
    try {
      const reopened = await second.sessions.open(id);
      assert.equal(reopened.resumed, true);
      assert.ok(reopened.messageCount >= 2);
      let context = "";
      h.faux.setResponses([(messages) => { context = JSON.stringify(messages); return fauxAssistantMessage("restored"); }]);
      second.sessions.prompt(id, "what did I say?");
      await second.sessions.wait(id);
      assert.match(context, /hello history/);
      assert.match(context, /remember me/);
    } finally { await second.shutdown(); }
  } finally { await h.close(); }
});
test("concurrent opens reserve capacity; duplicate/busy/missing sessions have named errors", async () => {
  const h = await harness([slow], { maxSessions: 1 });
  try {
    const [a, b] = await Promise.all([h.request("session.open", { cwd: h.dir }), h.request("session.open", { cwd: h.dir })]);
    const id = result<{ sessionId: string }>(a).sessionId;
    assert.ok(!b.ok && b.error.code === "SESSION_LIMIT");
    const duplicate = await h.request("session.open", { sessionId: id });
    assert.ok(!duplicate.ok && duplicate.error.code === "SESSION_ALREADY_OPEN");
    await h.request("prompt", { sessionId: id, text: "slow" });
    const busy = await h.request("prompt", { sessionId: id, text: "second" });
    assert.ok(!busy.ok && busy.error.code === "SESSION_BUSY");
    const missing = await h.request("close", { sessionId: "missing" });
    assert.ok(!missing.ok && missing.error.code === "SESSION_NOT_FOUND");
  } finally { await h.close(); }
});
test("steer and followUp share bounded queue and are delivered inside one host turn", async () => {
  let release!: (value: ReturnType<typeof fauxAssistantMessage>) => void;
  const contexts: string[] = [];
  const next: Steps[number] = (context) => {
    contexts.push(JSON.stringify(context));
    return fauxAssistantMessage("next");
  };
  const h = await harness([() => new Promise((resolve) => { release = resolve; }), next, next], { maxQueuedMessages: 2 });
  try {
    const id = await open(h);
    await h.request("prompt", { sessionId: id, text: "first" });
    await until(() => !!release);
    assert.equal(result<{ disposition: string }>(await h.request("steer", { sessionId: id, text: "steering" })).disposition, "queued");
    assert.equal(result<{ disposition: string }>(await h.request("followUp", { sessionId: id, text: "following" })).disposition, "queued");
    const full = await h.request("steer", { sessionId: id, text: "overflow" });
    assert.ok(!full.ok && full.error.code === "QUEUE_LIMIT");
    release(fauxAssistantMessage("initial"));
    await h.daemon.sessions.wait(id);
    assert.match(contexts.join(""), /steering/);
    assert.match(contexts.join(""), /following/);
    assert.equal(h.events.filter((e) => e.event === "turn.completed").length, 1);
  } finally { await h.close(); }
});
test("abort and timeout emit exactly one terminal status and allow another prompt", async () => {
  for (const timeout of [false, true]) {
    const h = await harness([slow, fauxAssistantMessage("recovered")], { turnTimeoutMs: timeout ? 40 : 5000 });
    try {
      const id = await open(h);
      await h.request("prompt", { sessionId: id, text: "wait" });
      await until(() => h.faux.state.callCount === 1);
      if (!timeout) await h.request("abort", { sessionId: id });
      await h.daemon.sessions.wait(id);
      assert.deepEqual(h.events.filter((e) => e.event === "turn.completed").map((e) => e.status), [timeout ? "timeout" : "aborted"]);
      assert.equal(h.events.some((e) => e.event === "error" && e.code === "TURN_TIMEOUT"), timeout);
      await h.request("prompt", { sessionId: id, text: "again" });
      await h.daemon.sessions.wait(id);
      assert.ok(h.events.some((e) => e.event === "text.delta" && e.text === "recovered"));
    } finally { await h.close(); }
  }
});
test("immediate cancellation during prompt preparation cannot start a later model call", async () => {
  const h = await harness([fauxAssistantMessage("unexpected")]);
  try {
    const id = await open(h);
    h.daemon.sessions.prompt(id, "cancel immediately");
    await h.daemon.sessions.abort(id);
    assert.equal(h.faux.state.callCount, 0);
    assert.deepEqual(h.events.filter((e) => e.event === "turn.completed").map((e) => e.status), ["aborted"]);
  } finally { await h.close(); }
});
test("idle sessions are reclaimed but running sessions stay alive until completion", async () => {
  const h = await harness([slow], { idleTimeoutMs: 30 });
  try {
    await open(h);
    await until(() => h.daemon.sessions.size === 0);
    const id = await open(h);
    await h.request("prompt", { sessionId: id, text: "slow" });
    await delay(60);
    assert.equal(h.daemon.sessions.size, 1);
    await h.request("abort", { sessionId: id });
    await until(() => h.daemon.sessions.size === 0);
  } finally { await h.close(); }
});

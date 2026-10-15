import assert from "node:assert/strict";
import { test } from "node:test";
import { bounded } from "./synchronization.ts";
import { fauxAssistantMessage, fauxToolCall } from "@earendil-works/pi-ai";
import { Type } from "typebox";
import { harness, result, until } from "./helpers.ts";

for (const command of ["abort", "close"]) {
  test(`${command} waiting on A keeps ping/models.list/B prompt responsive; shutdown drains queued commands`, async () => {
    let release!: () => void;
    let blockingDone = false;
    let queuedDone = false;
    let shutdownDone = false;
    const h = await harness([
      fauxAssistantMessage(fauxToolCall("stuck", {}), { stopReason: "toolUse" }),
      fauxAssistantMessage("B completed"),
    ], { tools: ["stuck"], extensions: { enabled: true } }, [(pi) => pi.registerTool({
      name: "stuck", label: "Stuck", description: "Wait even after abort", parameters: Type.Object({}),
      async execute() {
        await new Promise<void>((resolve) => { release = resolve; });
        return { content: [{ type: "text", text: "released" }], details: {} };
      },
    })]);
    try {
      const a = result<{ sessionId: string }>(await h.request("session.open", { cwd: h.dir })).sessionId;
      const b = result<{ sessionId: string }>(await h.request("session.open", { cwd: h.dir })).sessionId;
      await h.request("prompt", { sessionId: a, text: "block" });
      await until(() => !!release);
      const blocked = h.request(command, { sessionId: a }).then((response) => { blockingDone = true; return response; });
      const queued = h.request("prompt", { sessionId: a, text: "after cancellation" })
        .then((response) => { queuedDone = true; return response; });
      const fast = await bounded(Promise.all([h.request("ping"), h.request("models.list"),
        h.request("prompt", { sessionId: b, text: "independent" })]), "independent commands while A is blocked");
      assert.ok(fast.every((response) => response.ok));
      assert.equal(blockingDone, false);
      assert.equal(queuedDone, false);
      const shutdown = h.daemon.shutdown().then(() => { shutdownDone = true; });
      assert.equal((await h.request("ping")).ok, false); // Shutdown has entered its rejecting state.
      assert.equal(shutdownDone, false);
      release();
      assert.ok((await blocked).ok);
      const after = await queued;
      if (command === "abort") assert.ok(after.ok);
      else assert.ok(!after.ok && after.error.code === "SESSION_NOT_FOUND");
      await shutdown;
      assert.equal(h.daemon.sessions.size, 0);
      assert.equal(queuedDone, true);
    } finally { release?.(); await h.close(); }
  });
}

test("session.open resource loading does not block health checks and shutdown waits for it", async () => {
  const h = await harness();
  const original = h.daemon.sessions.open.bind(h.daemon.sessions);
  let release!: () => void;
  h.daemon.sessions.open = async (...args) => {
    await new Promise<void>((resolve) => { release = resolve; });
    return original(...args);
  };
  try {
    const opening = h.request("session.open", { cwd: h.dir });
    assert.ok((await h.request("ping")).ok);
    let stopped = false;
    const shutdown = h.daemon.shutdown().then(() => { stopped = true; });
    assert.equal((await h.request("ping")).ok, false); // Shutdown has entered its rejecting state.
    assert.equal(stopped, false);
    release();
    assert.ok((await opening).ok);
    await shutdown;
    assert.equal(h.daemon.sessions.size, 0);
  } finally { release?.(); await h.close(); }
});

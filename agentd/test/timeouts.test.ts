import assert from "node:assert/strict";
import { setImmediate as nextTurn } from "node:timers/promises";
import { test } from "node:test";
import { fauxAssistantMessage, fauxToolCall } from "@earendil-works/pi-ai";
import { Type } from "typebox";
import { harness, result, until } from "./helpers.ts";

for (const mode of ["idle", "max", "active"] as const) {
  test(`turn timeout policy: ${mode}`, async (t) => {
    let progress!: () => void;
    let finish!: () => void;
    let updates = 0;
    const h = await harness([
      fauxAssistantMessage(fauxToolCall("progress", {}), { stopReason: "toolUse" }),
      fauxAssistantMessage("finished"),
    ], { tools: ["progress"], extensions: { enabled: true }, turnIdleTimeoutMs: 100,
      turnMaxMs: mode === "max" ? 220 : 0 }, [(pi) => pi.registerTool({
      name: "progress", label: "Progress", description: "Report tool progress", parameters: Type.Object({}),
      async execute(_id, _args, signal, onUpdate) {
        progress = () => {
          if (mode !== "idle" && !signal?.aborted) {
            onUpdate?.({ content: [{ type: "text", text: "working" }], details: {} });
            updates++;
          }
        };
        await new Promise<void>((resolve) => {
          finish = resolve;
          signal?.addEventListener("abort", resolve.bind(null, undefined), { once: true });
          if (signal?.aborted) resolve();
        });
        return { content: [{ type: "text", text: "done" }], details: {} };
      },
    })]);
    try {
      const id = result<{ sessionId: string }>(await h.request("session.open", { cwd: h.dir })).sessionId;
      t.mock.timers.enable({ apis: ["setTimeout", "Date"], now: 1000 });
      const start = Date.now();
      await h.request("prompt", { sessionId: id, text: "long task" });
      await until(() => { t.mock.timers.tick(0); return !!finish; });
      for (let i = 0; i < 40; i++) {
        progress();
        await nextTurn(); // Deliver SDK progress before advancing the policy clock.
        t.mock.timers.tick(10);
      }
      if (mode === "active") finish();
      await until(() => {
        t.mock.timers.tick(0);
        return h.events.some((event) => event.event === "turn.completed");
      });
      await h.daemon.sessions.wait(id);
      const errors = h.events.filter((event) => event.event === "error");
      const terminal = h.events.filter((event) => event.event === "turn.completed");
      assert.equal(terminal.length, 1);
      if (mode === "active") {
        assert.ok(Date.now() - start > 350);
        assert.equal(updates, 40);
        assert.deepEqual(errors, []);
        assert.equal(terminal[0].status, "completed");
      } else {
        assert.deepEqual(errors.map((event) => event.code), [mode === "idle" ? "TURN_IDLE_TIMEOUT" : "TURN_MAX_TIMEOUT"]);
        assert.equal(terminal[0].status, "timeout");
        if (mode === "max") assert.ok(updates >= 10);
      }
    } finally { finish?.(); await h.close(); }
  });
}

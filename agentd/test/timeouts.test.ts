import assert from "node:assert/strict";
import { test } from "node:test";
import { setTimeout as delay } from "node:timers/promises";
import { fauxAssistantMessage, fauxToolCall } from "@earendil-works/pi-ai";
import { Type } from "typebox";
import { harness, result } from "./helpers.ts";

for (const mode of ["idle", "max", "active"] as const) {
  test(`turn timeout policy: ${mode}`, async () => {
    let updates = 0;
    const h = await harness([
      fauxAssistantMessage(fauxToolCall("progress", {}), { stopReason: "toolUse" }),
      fauxAssistantMessage("finished"),
    ], { tools: ["progress"], extensions: { enabled: true }, turnIdleTimeoutMs: 100,
      turnMaxMs: mode === "max" ? 220 : 0 }, [(pi) => pi.registerTool({
      name: "progress", label: "Progress", description: "Report tool progress", parameters: Type.Object({}),
      async execute(_id, _args, signal, onUpdate) {
        for (let i = 0; i < 40 && !signal?.aborted; i++) {
          await delay(10);
          if (mode !== "idle") {
            onUpdate?.({ content: [{ type: "text", text: "working" }], details: {} });
            updates++;
          }
        }
        return { content: [{ type: "text", text: "done" }], details: {} };
      },
    })]);
    try {
      const id = result<{ sessionId: string }>(await h.request("session.open", { cwd: h.dir })).sessionId;
      const start = performance.now();
      await h.request("prompt", { sessionId: id, text: "long task" });
      await h.daemon.sessions.wait(id);
      const errors = h.events.filter((event) => event.event === "error");
      const terminal = h.events.filter((event) => event.event === "turn.completed");
      assert.equal(terminal.length, 1);
      if (mode === "active") {
        assert.ok(performance.now() - start > 350);
        assert.equal(updates, 40);
        assert.deepEqual(errors, []);
        assert.equal(terminal[0].status, "completed");
      } else {
        assert.deepEqual(errors.map((event) => event.code), [mode === "idle" ? "TURN_IDLE_TIMEOUT" : "TURN_MAX_TIMEOUT"]);
        assert.equal(terminal[0].status, "timeout");
        if (mode === "max") assert.ok(updates >= 10);
      }
    } finally { await h.close(); }
  });
}

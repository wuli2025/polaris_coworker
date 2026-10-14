import assert from "node:assert/strict";
import { test } from "node:test";
import { existsSync } from "node:fs";
import { join } from "node:path";
import { fauxAssistantMessage, fauxToolCall } from "@earendil-works/pi-ai";
import { harness, result, until } from "./helpers.ts";

test("sandbox bash/write/edit only reach the protocol backend, never host filesystem", async () => {
  for (const name of ["bash", "write", "edit"]) {
    const h = await harness();
    try {
      const path = join(h.dir, "never-created");
      const args: Parameters<typeof fauxToolCall>[1] = name === "bash" ? { command: `touch '${path}'` } : name === "write"
        ? { path, content: "sandbox" } : { path, edits: [{ oldText: "before", newText: "after" }] };
      h.faux.setResponses([fauxAssistantMessage([fauxToolCall(name, args, { id: "sandbox-call" })]), fauxAssistantMessage("done")]);
      const { sessionId } = result<any>(await h.request("session.open", { cwd: h.dir,
        toolProfile: "full", permissionMode: "auto-all", sandbox: true }));
      await h.request("prompt", { sessionId, text: "execute in sandbox" });
      const handled = new Set<string>();
      await until(() => {
        for (const event of h.events) if (event.event === "sandbox.request" && !handled.has(event.callId)) {
          handled.add(event.callId);
          void h.request("sandbox.respond", { sessionId, callId: event.callId,
            output: event.operation === "read" ? Buffer.from("before").toString("base64") : "" });
        }
        return h.events.some((event) => event.event === "turn.completed");
      });
      assert.ok(handled.size > 0);
      assert.equal(existsSync(path), false);
      assert.ok(h.events.some((event) => event.event === "tool.completed" && !event.isError));
    } finally { await h.close(); }
  }
});
test("unavailable sandbox yields a tool error without falling back", async () => {
  const h = await harness([fauxAssistantMessage([fauxToolCall("write", { path: "no-host-write", content: "x" }, { id: "c" })]), fauxAssistantMessage("done")]);
  try {
    const { sessionId } = result<any>(await h.request("session.open", { cwd: h.dir, toolProfile: "full", permissionMode: "auto-all", sandbox: true }));
    await h.request("prompt", { sessionId, text: "write" });
    await until(() => h.events.some((event) => event.event === "sandbox.request"));
    const event = h.events.find((event) => event.event === "sandbox.request")!;
    if (event.event === "sandbox.request") await h.request("sandbox.respond", {
      sessionId, callId: event.callId, output: "", error: "SANDBOX_UNAVAILABLE",
    });
    await h.daemon.sessions.wait(sessionId);
    assert.ok(h.events.some((event) => event.event === "tool.completed" && event.isError));
    assert.equal(existsSync(join(h.dir, "no-host-write")), false);
  } finally { await h.close(); }
});

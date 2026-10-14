import assert from "node:assert/strict";
import { test } from "node:test";
import { fauxAssistantMessage, fauxToolCall } from "@earendil-works/pi-ai";
import type { ExtensionFactory } from "@earendil-works/pi-coding-agent";
import { Type } from "typebox";
import { Daemon } from "../src/daemon.ts";
import { memoryCredentials } from "../src/models/credentials.ts";
import type { ProtocolEvent } from "../src/protocol/events.ts";
import { harness, result } from "./helpers.ts";

const echo: ExtensionFactory = (pi) => pi.registerTool({
  name: "echo", label: "Echo", description: "Echo text", parameters: Type.Object({ text: Type.String() }),
  async execute(_id, args) { return { content: [{ type: "text", text: args.text }], details: {} }; },
});

for (const registration of ["startup", "apply"] as const) {
  test(`${registration} credentials redact encoded keys in real logs and events`, async () => {
    const key = 'sk+/"\\private=42';
    const variants = [key, encodeURIComponent(key), Buffer.from(key).toString("base64"),
      JSON.stringify(key).slice(1, -1)];
    const h = await harness(variants.flatMap((text) => [
      fauxAssistantMessage(fauxToolCall("echo", { text }), { stopReason: "toolUse" }),
      fauxAssistantMessage("done"),
    ]), { tools: ["echo"], extensions: { enabled: true } }, [echo]);
    const credentials = memoryCredentials();
    const provider = h.daemon.config.defaultModel.provider;
    if (registration === "startup") {
      await credentials.modify(provider, async () => ({ type: "api_key", key }));
    }
    const events: ProtocolEvent[] = [];
    const logs: string[] = [];
    const daemon = await Daemon.create(h.daemon.config, (event) => {
      if (event.type === "event") events.push(event);
    }, (line) => logs.push(line), { credentials, factories: [echo] });
    try {
      if (registration === "apply") {
        result(await daemon.handle({ id: 1, method: "config.apply", params: { credentials: { [provider]: key } } }));
      }
      const { sessionId } = result<{ sessionId: string }>(await daemon.handle({
        id: 2, method: "session.open", params: { cwd: h.dir },
      }));
      for (const variant of variants) {
        daemon.logger.log("warn", variant, { sessionId: variant });
        const logged = JSON.parse(logs.at(-1)!);
        assert.equal(logged.message, "[REDACTED]");
        assert.equal(logged.sessionId, "[REDACTED]");
        events.length = 0;
        result(await daemon.handle({ id: 3, method: "prompt", params: { sessionId, text: "echo" } }));
        await daemon.sessions.wait(sessionId);
        // Tool payloads carry a complete credential representation in one event.
        const started = events.find((e) => e.event === "tool.started");
        assert.deepEqual(started?.args, { text: "[REDACTED]" });
        const completed = events.find((e) => e.event === "tool.completed");
        assert.match(JSON.stringify(completed?.result), /\[REDACTED\]/);
        assert.ok(!JSON.stringify(events).includes(JSON.stringify(variant).slice(1, -1)));
        assert.equal(events.at(-1)?.event, "turn.completed");
      }
    } finally {
      await daemon.shutdown();
      await h.close();
    }
  });
}

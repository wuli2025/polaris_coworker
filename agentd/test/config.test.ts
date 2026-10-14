import assert from "node:assert/strict";
import { writeFileSync, existsSync } from "node:fs";
import { join } from "node:path";
import { test } from "node:test";
import { fauxAssistantMessage } from "@earendil-works/pi-ai";
import { Type } from "typebox";
import { loadConfig, mergeConfig } from "../src/config/load.ts";
import { Logger } from "../src/log.ts";
import { harness, result } from "./helpers.ts";

test("defaults ← file ← runtime overrides; two configurations change session assembly", async () => {
  const h = await harness([], {}, [(pi) => pi.registerTool({
    name: "probe", label: "Probe", description: "Probe", parameters: Type.Object({}),
    async execute() { return { content: [{ type: "text", text: "probe" }], details: {} }; },
  })]);
  try {
    const path = join(h.dir, "config.json");
    writeFileSync(path, JSON.stringify({ maxSessions: 1, thinkingLevel: "off" }));
    const fromFile = loadConfig(path);
    assert.equal(fromFile.maxSessions, 1);
    assert.equal(mergeConfig(fromFile, { maxSessions: 2 }).maxSessions, 2);
    for (const [index, settings] of [
      { maxSessions: 1, maxQueuedMessages: 0, idleTimeoutMs: 10000, turnIdleTimeoutMs: 2000,
        maxLineBytes: 128, tools: ["read"], thinkingLevel: "off", logLevel: "error", extensions: { enabled: false } },
      { maxSessions: 3, maxQueuedMessages: 5, idleTimeoutMs: 20000, turnIdleTimeoutMs: 4000,
        maxLineBytes: 2048, tools: ["probe"], thinkingLevel: "high", logLevel: "debug", extensions: { enabled: true } },
    ].entries()) {
      const sessionDir = join(h.dir, `sessions-${index}`);
      const applied = await h.request("config.apply", { config: { ...settings, sessionDir } });
      assert.ok(applied.ok);
      for (const [key, value] of Object.entries(settings)) {
        assert.deepEqual(h.daemon.config[key as keyof typeof h.daemon.config], value);
      }
      const opened = result<{ sessionId: string; thinkingLevel: string; tools: string[] }>(
        await h.request("session.open", { cwd: h.dir }));
      assert.equal(opened.thinkingLevel, settings.thinkingLevel);
      assert.deepEqual(opened.tools, settings.tools);
      assert.ok(existsSync(sessionDir));
      await h.request("close", { sessionId: opened.sessionId });
    }
  } finally { await h.close(); }
});
test("invalid config is named, does not mutate state, and providers/keys can be applied and removed", async () => {
  const h = await harness();
  try {
    const before = h.daemon.config;
    for (const config of [{ maxSessions: 0 }, { tools: "bash" }, { secret: "bad" }, { thinkingLevel: "huge" }]) {
      const response = await h.request("config.apply", { config });
      assert.ok(!response.ok && response.error.code === "INVALID_CONFIG");
      assert.equal(h.daemon.config, before);
    }
    const provider = { ...before.providers[0], id: "second-provider" };
    assert.ok((await h.request("config.apply", { config: { providers: [provider] },
      credentials: { "second-provider": "new-private-key" } })).ok);
    const list = result<{ models: { provider: string }[] }>(await h.request("models.list"));
    assert.ok(list.models.some((m) => m.provider === "second-provider"));
    assert.deepEqual(await h.daemon.models.credentials.read("second-provider"), { type: "api_key", key: "new-private-key" });
    assert.ok((await h.request("config.apply", { credentials: { "second-provider": null } })).ok);
    assert.equal(await h.daemon.models.credentials.read("second-provider"), undefined);
  } finally { await h.close(); }
});
test("keys stay out of structured logs and emitted events, including after rotation", async () => {
  const oldKey = "test-secret-key-7654321";
  const newKey = "new-secret-key-1234567";
  const h = await harness([fauxAssistantMessage(oldKey)], { logLevel: "debug" });
  try {
    const provider = h.daemon.config.defaultModel.provider;
    const response = await h.request("config.apply", { credentials: { [provider]: newKey } });
    assert.ok(response.ok);
    const id = result<{ sessionId: string }>(await h.request("session.open", { cwd: h.dir })).sessionId;
    await h.request("prompt", { sessionId: id, text: newKey });
    await h.daemon.sessions.wait(id);
    const logger = new Logger(() => "debug", h.daemon.models.redactor, (line) => h.logs.push(line));
    logger.log("error", `${oldKey} ${newKey}`, { sessionId: id, requestId: "r" });
    const output = h.logs.join("") + JSON.stringify(h.events) + JSON.stringify(response);
    assert.ok(!output.includes(oldKey));
    assert.ok(!output.includes(newKey));
    for (const line of h.logs) assert.ok(JSON.parse(line).level);
    assert.ok(h.logs.some((line) => JSON.parse(line).sessionId === id));
  } finally { await h.close(); }
});

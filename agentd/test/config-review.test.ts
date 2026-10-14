import assert from "node:assert/strict";
import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { test } from "node:test";
import { createReadOnlyTools } from "@earendil-works/pi-coding-agent";
import { loadConfig, mergeConfig } from "../src/config/load.ts";
import { DaemonError, publicError } from "../src/protocol/errors.ts";
import { createSession } from "../src/sessions/create.ts";
import { harness, result } from "./helpers.ts";

test("retry and compaction defaults and overrides reach actual Pi session SettingsManager", async () => {
  const defaults = loadConfig();
  assert.equal(defaults.turnIdleTimeoutMs, 180000);
  assert.equal(defaults.turnMaxMs, 0);
  assert.equal(defaults.retry.enabled, true);
  assert.equal(defaults.compaction.enabled, true);
  const h = await harness();
  try {
    for (const retry of [defaults.retry, { enabled: true, maxRetries: 7, baseDelayMs: 123, maxAgentDelayMs: 4567 }]) {
      const config = mergeConfig(h.daemon.config, { retry });
      const session = await createSession(config, h.daemon.models, [], undefined, h.dir);
      try {
        assert.deepEqual(session.settingsManager.getRetrySettings(), retry);
        assert.equal(session.settingsManager.getCompactionEnabled(), true);
      } finally { session.dispose(); }
    }
  } finally { await h.close(); }
});

test("default allowlist matches SDK read-only tools; write/exec requires explicit host configuration", async () => {
  const tools = loadConfig().tools;
  const h = await harness([], { tools });
  try {
    assert.deepEqual(tools, createReadOnlyTools(h.dir).map((tool) => tool.name));
    const opened = result<{ sessionId: string; tools: string[] }>(await h.request("session.open", { cwd: h.dir }));
    assert.deepEqual(opened.tools, tools);
    assert.ok(!opened.tools.some((name) => ["bash", "powershell", "edit", "write"].includes(name)));
    await h.request("close", { sessionId: opened.sessionId });
    await h.request("config.apply", { config: { tools: ["read", "bash", "edit", "write"] } });
    const explicit = result<{ tools: string[] }>(await h.request("session.open", { cwd: h.dir }));
    assert.deepEqual(explicit.tools, ["read", "bash", "edit", "write"]);
  } finally { await h.close(); }
});

test("file and runtime validation return only failing JSON Pointer paths, preserving all nested errors", async () => {
  const h = await harness();
  try {
    const invalid = { maxSessions: 0, retry: { enabled: "private-retry", baseDelayMs: "private-delay" },
      providers: [{ ...h.daemon.config.providers[0], baseUrl: 42 }], unexpected: "private-extra" };
    const expected = ["/maxSessions", "/providers/0/baseUrl", "/retry/baseDelayMs", "/retry/enabled", "/unexpected"];
    const response = await h.request("config.apply", { config: invalid });
    assert.ok(!response.ok);
    assert.deepEqual(response.error.paths, expected);
    const path = join(h.dir, "bad-config.json");
    writeFileSync(path, JSON.stringify(invalid));
    assert.throws(() => loadConfig(path), (error: unknown) => {
      assert.ok(error instanceof DaemonError);
      assert.deepEqual(error.paths, expected);
      assert.ok(!JSON.stringify(publicError(error)).includes("private-"));
      return true;
    });
    assert.ok(!JSON.stringify(response).includes("private-"));
    assert.throws(() => mergeConfig(h.daemon.config, { retry: {} }), (error: unknown) => {
      assert.ok(error instanceof DaemonError);
      assert.deepEqual(error.paths, ["/retry/enabled"]);
      return true;
    });
    assert.throws(() => mergeConfig(h.daemon.config, { providers: Array(2).fill(h.daemon.config.providers[0]) }),
      (error: unknown) => error instanceof DaemonError && error.paths?.[0] === "/providers");
    for (const pathToLoad of [path, join(h.dir, "missing-secret-path")]) {
      writeFileSync(path, '{"private-secret":');
      assert.throws(() => loadConfig(pathToLoad), (error: unknown) => {
        assert.ok(error instanceof DaemonError);
        assert.deepEqual(error.paths, ["/"]);
        assert.ok(!JSON.stringify(publicError(error)).includes("secret"));
        assert.match(error.message, /[\u4e00-\u9fff]/);
        return true;
      });
    }
  } finally { await h.close(); }
});

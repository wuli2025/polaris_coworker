import assert from "node:assert/strict";
import { test } from "node:test";
import { fauxAssistantMessage, InMemoryCredentialStore } from "@earendil-works/pi-ai";
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { harness, result } from "./helpers.ts";
import { recordUsage } from "../src/events/usage-store.ts";

const unknown = { id: "polaris-out-of-catalog-test", name: "polaris-out-of-catalog-test", contextWindow: 128000, maxTokens: 8192,
  input: ["text"], reasoning: false, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 } };

test("native providers register out-of-catalog models without custom endpoints and retain OAuth", async () => {
  const credentials = new InMemoryCredentialStore();
  const h = await harness([], {}, [], credentials);
  try {
    for (const [id, api] of [["openai-codex", "openai-codex-responses"], ["anthropic", "anthropic-messages"]]) {
      const credential = { type: "oauth" as const, access: "test-access", refresh: "test-refresh", expires: Date.now() + 3600000 };
      await credentials.modify(id, async () => credential);
      const config = { providers: [{ id, models: [unknown] }], defaultModel: { provider: id, id: unknown.id } };
      const before = h.daemon.models.runtime;
      result(await h.request("config.validate", { config }));
      assert.equal(h.daemon.models.runtime, before, "save validation must not change the active runtime");
      result(await h.request("config.apply", { config }));
      const model = h.daemon.models.runtime.getModel(id, unknown.id);
      assert.ok(model);
      assert.equal(model.api, api);
      assert.ok(model.baseUrl.startsWith("https://"));
      assert.deepEqual(await credentials.read(id), credential);
      assert.equal(h.daemon.models.runtime.isUsingOAuth(id), true);
      assert.ok(!h.daemon.models.builtinCatalog.some((m) => m.provider === id && m.id === unknown.id));
      const message = { ...fauxAssistantMessage("usage"), provider: id, model: unknown.id };
      const directory = join(h.dir, id);
      recordUsage(directory, "session", "turn", 1, message, h.daemon.models.runtime);
      const date = readdirSync(directory)[0];
      const row = JSON.parse(readFileSync(join(directory, date, readdirSync(join(directory, date))[0]), "utf8"));
      assert.equal(row.priceAvailable, false);
    }
  } finally { await h.close(); }
});

test("validation and apply report the same field paths, accept catalog tiered costs", async () => {
  const h = await harness();
  try {
    for (const method of ["config.validate", "config.apply"]) {
      const response = await h.request(method, { config: { providers: [
        { id: "openai-codex", models: [{ ...unknown, maxTokens: 0 }] },
      ] } });
      assert.equal(response.ok, false);
      if (response.ok) throw new Error("invalid config accepted");
      assert.equal(response.error.code, "INVALID_CONFIG");
      assert.deepEqual(response.error.paths, ["/providers/0/models/0/maxTokens"]);
      result(await h.request(method, { config: { providers: [{ id: "anthropic", models: [{ ...unknown,
        cost: { ...unknown.cost, input: 3, tiers: [{ ...unknown.cost, input: 6, inputTokensAbove: 200000 }] },
      }] }] } }));
    }
  } finally { await h.close(); }
});

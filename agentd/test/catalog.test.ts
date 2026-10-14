import assert from "node:assert/strict";
import { test } from "node:test";
import { readFile } from "node:fs/promises";
import { Value } from "typebox/value";
import { CatalogSchema } from "../src/protocol/schema.ts";
import { harness, result } from "./helpers.ts";

test("apply rebuilds builtin catalog with updated native provider configuration", async () => {
  const h = await harness();
  try {
    const builtin = h.daemon.models.builtinCatalog.find((m) => m.provider === "anthropic");
    assert.ok(builtin);
    const model = { ...builtin, name: "Updated built-in", contextWindow: 64000, maxTokens: 1000 };
    const { provider, api, baseUrl, ...configModel } = model;
    result(await h.request("config.apply", { config: { providers: [{ id: provider, api, baseUrl, models: [configModel] }] } }));
    const catalog = result<{ models: typeof model[] }>(await h.request("models.catalog"));
    assert.ok(Value.Check(CatalogSchema, catalog));
    assert.equal(catalog.models.find((m) => m.provider === provider)?.name, "Updated built-in");
    assert.equal(catalog.models.find((m) => m.provider === provider)?.contextWindow, 64000);
  } finally { await h.close(); }
});
test("shared catalog sample validates and incomplete/invalid metadata is rejected", async () => {
  const text = await readFile(new URL("../../src-tauri/crates/polaris-kernel/tests/fixtures/protocol-samples.jsonl", import.meta.url), "utf8");
  const sample = text.trim().split("\n").map((line) => JSON.parse(line)).find((s) => s.id === "catalog-v6").result;
  assert.ok(Value.Check(CatalogSchema, sample));
  assert.ok(!Value.Check(CatalogSchema, { models: [{ provider: "p", id: "m" }] }));
  const invalid = { models: sample.models.map((m) => ({ ...m, cost: { ...m.cost, input: "invalid" } })) };
  assert.ok(!Value.Check(CatalogSchema, invalid));
});

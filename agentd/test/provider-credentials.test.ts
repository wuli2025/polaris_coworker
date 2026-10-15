import assert from "node:assert/strict";
import { test } from "node:test";
import { mkdtemp, readFile, rm, writeFile, stat, open, readdir } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { ModelRuntime } from "@earendil-works/pi-coding-agent";
import { FileCredentials } from "../src/models/file-credentials.ts";
import { until } from "./helpers.ts";
import { Redactor } from "../src/models/credentials.ts";

test("SDK refresh persists and serializes across store instances with fake clock/endpoint", async (t) => {
  const dir = await mkdtemp(join(tmpdir(), "p4-refresh-"));
  const path = join(dir, "providers.json");
  const redactor = new Redactor();
  const stores = [new FileCredentials(path, redactor), new FileCredentials(path, redactor)];
  await writeFile(path, JSON.stringify({ items: [{ id: "untouched" }] }));
  const now = 1800000000000;
  t.mock.method(Date, "now", () => now);
  await stores[0].modify("anthropic", async () => ({ type: "oauth", access: "old-access-secret", refresh: "old-refresh-secret", expires: now - 1 }));
  let release!: () => void;
  const gate = new Promise<void>((resolve) => { release = resolve; });
  let attempted = 0;
  for (const store of stores) {
    const modify = store.modify.bind(store);
    t.mock.method(store, "modify", (...args: Parameters<typeof modify>) => {
      attempted++;
      return modify(...args);
    });
  }
  let refreshes = 0;
  t.mock.method(globalThis, "fetch", async (input: unknown, options?: RequestInit) => {
    assert.match(String(input), /oauth\/token/);
    refreshes++;
    assert.match(String(options?.body), /old-refresh-secret/);
    await gate;
    return new Response(JSON.stringify({ access_token: "new-access-secret", refresh_token: "new-refresh-secret", expires_in: 3600 }),
      { headers: { "content-type": "application/json" } });
  });
  try {
    const runtimes = await Promise.all(stores.map((credentials) => ModelRuntime.create({ credentials,
      modelsPath: null, allowModelNetwork: false, refreshOnCreate: false })));
    const before = await readFile(path, "utf8");
    const oldFile = await open(path, "r");
    t.after(() => oldFile.close());
    const pending = Promise.all(runtimes.map((runtime) => runtime.getAuth("anthropic")));
    await until(() => attempted === 2 && refreshes === 1);
    release();
    const results = await pending;
    assert.equal(refreshes, 1);
    const disk = JSON.parse(await readFile(path, "utf8"));
    assert.equal(disk.credentials.anthropic.access, "new-access-secret");
    assert.equal(disk.credentials.anthropic.refresh, "new-refresh-secret");
    if (process.platform !== "win32") {
      assert.equal((await stat(path)).mode & 0o777, 0o600);
      assert.equal(await oldFile.readFile("utf8"), before, "refresh replaces the file atomically");
    }
    assert.deepEqual(await readdir(dir), ["providers.json"]);
    assert.ok(results.every((result) => result?.auth.apiKey === "new-access-secret"));
    const stored = await new FileCredentials(path, redactor).read("anthropic");
    if (stored?.type !== "oauth") throw new Error("Missing OAuth credential");
    assert.equal(stored.refresh, "new-refresh-secret");
    assert.ok(stored.expires > now);
    assert.equal(redactor.text("old-access-secret new-access-secret new-refresh-secret"), "[REDACTED] [REDACTED] [REDACTED]");
    assert.deepEqual(JSON.parse(await readFile(path, "utf8")).items, [{ id: "untouched" }]);
  } finally { release(); await rm(dir, { recursive: true, force: true }); }
});

test("login waits for refresh; corrupt storage is never overwritten", async () => {
  const dir = await mkdtemp(join(tmpdir(), "p4-login-write-"));
  const path = join(dir, "providers.json");
  const a = new FileCredentials(path, new Redactor());
  const b = new FileCredentials(path, new Redactor());
  let release!: () => void;
  let entered!: () => void;
  const enteredPromise = new Promise<void>((r) => { entered = r; });
  const refreshing = a.modify("p", async () => {
    entered(); await new Promise<void>((r) => { release = r; });
    return { type: "oauth", access: "refreshed", refresh: "rotated", expires: 10 };
  });
  try {
    await enteredPromise;
    const login = b.modify("p", async (current) => {
      assert.equal(current?.type === "oauth" && current.refresh, "rotated");
      return { type: "api_key", key: "new-login" };
    });
    release();
    await Promise.all([refreshing, login]);
    assert.deepEqual(await a.read("p"), { type: "api_key", key: "new-login" });
    await writeFile(path, "broken JSON");
    await assert.rejects(b.modify("p", async () => ({ type: "api_key", key: "lost" })), /CredentialStoreReadFailed/);
    assert.equal(await readFile(path, "utf8"), "broken JSON");
  } finally { release?.(); await rm(dir, { recursive: true, force: true }); }
});

test("native API registrations resolve credentials from the shared store", async () => {
  const { Models } = await import("../src/models/runtime.ts");
  const { loadConfig, mergeConfig } = await import("../src/config/load.ts");
  const dir = await mkdtemp(join(tmpdir(), "p4-protocols-"));
  const redactor = new Redactor();
  const credentials = new FileCredentials(join(dir, "providers.json"), redactor);
  try {
    const model = { id: "m", name: "M", input: ["text" as const], reasoning: false,
      contextWindow: 10000, maxTokens: 1000, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 } };
    const providers = ["anthropic-messages", "openai-completions", "openai-responses"].map((api, i) => ({
      id: `custom-${i}`, api, authHeader: i === 0, baseUrl: "https://example.test", models: [model],
    }));
    for (const provider of providers) await credentials.modify(provider.id, async () => ({ type: "api_key", key: `secret-${provider.id}` }));
    const models = await Models.create(mergeConfig(loadConfig(), { providers }), credentials, redactor);
    assert.ok(!models.builtinCatalog.some((model) => model.provider.startsWith("custom-")));
    assert.ok(models.builtinCatalog.some((model) => model.provider === "anthropic"));
    for (const provider of providers) {
      assert.equal(models.runtime.getModel(provider.id, "m")?.api, provider.api);
      const auth = (await models.runtime.getAuth(provider.id))?.auth;
      assert.equal(auth?.apiKey, `secret-${provider.id}`);
      if (provider.authHeader) assert.equal(auth?.headers?.Authorization, `Bearer secret-${provider.id}`);
    }
    const builtin = models.runtime.getModels("anthropic")[0];
    await models.apply(mergeConfig(loadConfig(), { providers: [{ id: "anthropic", api: builtin.api,
      baseUrl: builtin.baseUrl, models: [model] }] }));
    assert.ok(models.runtime.getProvider("anthropic")?.auth.oauth, "translation must preserve native subscription auth");
  } finally { await rm(dir, { recursive: true, force: true }); }
});

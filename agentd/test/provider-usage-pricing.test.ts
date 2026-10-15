import assert from "node:assert/strict";
import { test } from "node:test";
import { mkdtempSync, readdirSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fauxAssistantMessage } from "@earendil-works/pi-ai";
import { registerFauxProvider } from "@earendil-works/pi-ai/compat";
import { recordUsage } from "../src/events/usage-store.ts";
import { harness, providerConfig, result, until } from "./helpers.ts";

function readRows(directory: string) {
  return readdirSync(directory).flatMap((date) => readdirSync(join(directory, date))
    .map((file) => JSON.parse(readFileSync(join(directory, date, file), "utf8"))));
}

test("usage pricing follows each model rate, independently of the request cost", () => {
  const dir = mkdtempSync(join(tmpdir(), "usage-pricing-"));
  const faux = registerFauxProvider({ models: [{ id: "priced-test" }] });
  const model = faux.getModel();
  const zero = { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 };
  try {
    const definitions = [undefined, zero,
      ...Object.keys(zero).map((key) => ({ ...zero, [key]: 2 })),
      { ...zero, input: NaN, output: 2 }, { ...zero, input: Infinity },
      { ...zero, input: -1, output: 2 }];
    let ordinal = 0;
    for (const cost of definitions) {
      for (const total of [0, 3]) {
        const message = { ...fauxAssistantMessage("private"), provider: model.provider, model: model.id };
        message.usage.cost.total = total;
        const runtime = { getModel(provider: string, id: string) {
          assert.equal(provider, model.provider);
          assert.equal(id, model.id);
          return cost ? { ...model, cost } : undefined;
        } };
        recordUsage(dir, "session", "turn", ordinal++, message, runtime);
      }
    }
    const rows = readRows(dir);
    assert.equal(rows.length, definitions.length * 2);
    for (let index = 0; index < definitions.length; index++) {
      for (let request = 0; request < 2; request++) {
        const row = rows.find((row) => row.requestId === `session:turn:${index * 2 + request}`);
        assert.ok(row);
        assert.equal(row.priceAvailable, index >= 2 && index < 6);
        assert.equal(row.usage.cost.total, request === 0 ? 0 : 3);
      }
    }
  } finally {
    faux.unregister();
    rmSync(dir, { recursive: true, force: true });
  }
});

for (const priced of [true, false]) {
  test(`session usage persists priceAvailable=${priced} from ModelRuntime`, async () => {
    const h = await harness([fauxAssistantMessage("zero request cost")]);
    try {
      const provider = providerConfig(h.faux.getModel());
      provider.models[0].cost = { input: priced ? 2 : 0, output: 0, cacheRead: 0, cacheWrite: 0 };
      const usageDir = join(h.dir, "usage");
      result(await h.request("config.apply", { config: { usageDir, providers: [provider] } }));
      const { sessionId } = result<{ sessionId: string }>(await h.request("session.open", { cwd: h.dir }));
      result(await h.request("prompt", { sessionId, text: "record usage" }));
      await h.daemon.sessions.wait(sessionId);
      const rows = readRows(usageDir);
      assert.equal(rows.length, 1);
      assert.equal(rows[0].priceAvailable, priced);
      assert.equal(rows[0].usage.cost.total, 0);
      assert.equal(rows[0].provider, provider.id);
      assert.equal(rows[0].model, provider.models[0].id);
    } finally { await h.close(); }
  });
}

test("an in-flight session records its own pricing after configuration replaces ModelRuntime", async () => {
  let release: ((message: ReturnType<typeof fauxAssistantMessage>) => void) | undefined;
  const h = await harness([() => new Promise((resolve) => { release = resolve; })]);
  try {
    const provider = providerConfig(h.faux.getModel());
    provider.models[0].cost = { input: 2, output: 0, cacheRead: 0, cacheWrite: 0 };
    const usageDir = join(h.dir, "usage");
    result(await h.request("config.apply", { config: { usageDir, providers: [provider] } }));
    const { sessionId } = result<{ sessionId: string }>(await h.request("session.open", { cwd: h.dir }));
    result(await h.request("prompt", { sessionId, text: "wait for configuration" }));
    await until(() => !!release);
    const replacement = providerConfig(h.faux.getModel());
    replacement.models[0].cost = { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 };
    result(await h.request("config.apply", { config: { providers: [replacement] } }));
    assert.equal(h.daemon.models.runtime.getModel(provider.id, provider.models[0].id)?.cost.input, 0);
    assert.ok(release);
    release(fauxAssistantMessage("finished on original runtime"));
    await h.daemon.sessions.wait(sessionId);
    const rows = readRows(usageDir);
    assert.equal(rows.length, 1);
    assert.equal(rows[0].priceAvailable, true);
  } finally {
    release?.(fauxAssistantMessage("cleanup"));
    await h.close();
  }
});

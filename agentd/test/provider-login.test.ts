import assert from "node:assert/strict";
import { test } from "node:test";
import { harness, result } from "./helpers.ts";

test("OAuth interaction bridge persists SDK login and never returns credential data", async () => {
  const h = await harness();
  try {
    const native = h.daemon.models.runtime.getProvider("anthropic");
    assert.ok(native?.auth.oauth);
    h.daemon.models.runtime.registerNativeProvider({ ...native, auth: { ...native.auth,
      oauth: { ...native.auth.oauth, async login(interaction) {
        interaction.notify({ type: "auth_url", url: "https://example.test/authorize" });
        const code = await interaction.prompt({ type: "manual_code", message: "Paste code" });
        assert.equal(code, "one-time-code");
        return { type: "oauth", access: "private-access", refresh: "private-refresh", expires: Date.now() + 3600000 };
      } } } });
    const started = result<{url:string}>(await h.request("provider.auth", { provider: "anthropic", operation: "start" }));
    assert.equal(started.url, "https://example.test/authorize");
    const finished = result(await h.request("provider.auth", { provider: "anthropic", operation: "respond", input: "one-time-code" }));
    assert.deepEqual(finished, { status: "ok" });
    assert.equal((await h.daemon.models.credentials.read("anthropic"))?.type, "oauth");
    assert.ok(!JSON.stringify([started, finished, h.logs, h.events]).includes("private-"));
    assert.deepEqual(result(await h.request("provider.auth", { provider: "anthropic", operation: "cancel" })), { status: "idle" });
  } finally { await h.close(); }
});

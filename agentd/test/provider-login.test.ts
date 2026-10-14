import assert from "node:assert/strict";
import { test } from "node:test";
import { mkdtemp, readFile, rm, stat, readdir, open } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { FileCredentials } from "../src/models/file-credentials.ts";
import { Redactor } from "../src/models/credentials.ts";
import { harness, result } from "./helpers.ts";

test("OAuth interaction bridge persists SDK login and never returns credential data", async () => {
  const dir = await mkdtemp(join(tmpdir(), "r5-oauth-login-"));
  const path = join(dir, "providers.json");
  const h = await harness([], {}, [], new FileCredentials(path, new Redactor()));
  const before = await readFile(path, "utf8");
  const oldFile = await open(path, "r");
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
    const disk = JSON.parse(await readFile(path, "utf8"));
    assert.equal(disk.credentials.anthropic.access, "private-access");
    assert.equal(disk.credentials.anthropic.refresh, "private-refresh");
    if (process.platform !== "win32") {
      assert.equal((await stat(path)).mode & 0o777, 0o600);
      assert.equal(await oldFile.readFile("utf8"), before, "rename preserves the old open inode");
    }
    assert.deepEqual(await readdir(dir), ["providers.json"]);
    assert.ok(!JSON.stringify([started, finished, h.logs, h.events]).includes("private-"));
    assert.deepEqual(result(await h.request("provider.auth", { provider: "anthropic", operation: "cancel" })), { status: "idle" });
  } finally { await oldFile.close(); await h.close(); await rm(dir, { recursive: true, force: true }); }
});

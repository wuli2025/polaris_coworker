import assert from "node:assert/strict";
import { test } from "node:test";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fauxAssistantMessage } from "@earendil-works/pi-ai";
import { ModelRuntime } from "@earendil-works/pi-coding-agent";
import { FileCredentials } from "../src/models/file-credentials.ts";
import { Redactor } from "../src/models/redactor.ts";
import { Logger } from "../src/log.ts";
import { modelError } from "../src/events/model-error.ts";

test("bundled credential authenticates without persistence, lists no key and redacts logs", async () => {
  const dir = await mkdtemp(join(tmpdir(), "bundled-provider-"));
  try {
    const path = join(dir, "providers.json");
    await writeFile(path, JSON.stringify({ current_id: "minimax", items: [] }));
    const before = await readFile(path, "utf8");
    const key = "test-bundled-private-value";
    const redactor = new Redactor();
    const store = new FileCredentials(path, redactor, undefined, { id: "minimax", key });
    const runtime = await ModelRuntime.create({ credentials: store, modelsPath: null, allowModelNetwork: false });
    assert.equal((await runtime.getAuth("minimax"))?.auth.apiKey, key);
    assert.equal(await readFile(path, "utf8"), before);
    assert.equal(JSON.stringify(await store.list()).includes(key), false);
    const logs: string[] = [];
    const logger = new Logger(() => "info", redactor, (line) => logs.push(line));
    logger.log("info", key, { code: key });
    assert.equal(logs.join("").includes(key), false);
    assert.match(logs.join(""), /REDACTED/);
    await store.modify("minimax", async () => ({ type: "api_key", key: "user-key" }));
    assert.deepEqual(await store.read("minimax"), { type: "api_key", key: "user-key" });
    assert.equal((await readFile(path, "utf8")).includes(key), false);
    const error = fauxAssistantMessage("", { stopReason: "error" });
    error.errorMessage = `model not supported for this account; ${key}; Authorization: Bearer secret`;
    const result = redactor.clean(modelError(error));
    assert.match(result.message, /model not supported for this account/);
    assert.equal(result.message.includes(key), false);
    assert.equal(result.message.includes("Bearer secret"), false);
  } finally { await rm(dir, { recursive: true, force: true }); }
});

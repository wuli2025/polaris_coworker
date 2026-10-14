import assert from "node:assert/strict";
import { test } from "node:test";
import { fork, type ChildProcess } from "node:child_process";
import { once } from "node:events";
import { mkdtemp, readFile, rm, mkdir, writeFile, utimes } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { acquire, reclaim } from "../src/models/credential-lock.ts";
import { reclaimable } from "../src/models/lock-owner.ts";
import { FileCredentials } from "../src/models/file-credentials.ts";
import { Redactor } from "../src/models/credentials.ts";

const fixture = new URL("../../src-tauri/crates/polaris-kernel/tests/fixtures/credential-lock-vectors.json", import.meta.url);
test("Node and Rust share reclamation decisions", async () => {
  for (const row of JSON.parse(await readFile(fixture, "utf8"))) {
    assert.equal(reclaimable(row.owner, row.host, row.process, row.now, row.maxHoldMs), row.reclaimable, row.name);
  }
});
function child(mode: string, path: string): ChildProcess {
  return fork(new URL("./fixtures/credential-lock-child.ts", import.meta.url), [mode, path],
    { execArgv: ["--import", "tsx"], stdio: ["ignore", "ignore", "inherit", "ipc"] });
}
async function killed(path: string) {
  const holder = child("hold", path);
  try { assert.deepEqual(await once(holder, "message"), ["held", undefined]); }
  finally { const exit = once(holder, "exit"); holder.kill("SIGKILL"); await exit; }
}
test("SIGKILL holder is recovered by a different process before wait timeout", { timeout: 10000 }, async () => {
  const dir = await mkdtemp(join(tmpdir(), "r5-kill-"));
  try {
    const path = join(dir, "providers.json");
    await killed(path);
    const writer = child("write", path);
    const exit = once(writer, "exit");
    assert.equal((await once(writer, "message"))[0], "written");
    assert.equal((await exit)[0], 0);
    assert.equal(JSON.parse(await readFile(path, "utf8")).credentials.p.key, "recovered");
  } finally { await rm(dir, { recursive: true, force: true }); }
});
test("two process reapers have exactly one winner; delayed release cannot delete replacement", { timeout: 10000 }, async () => {
  const dir = await mkdtemp(join(tmpdir(), "r5-reap-"));
  const children: ChildProcess[] = [];
  try {
    const path = join(dir, "providers.json");
    await killed(path);
    children.push(child("reap", path), child("reap", path));
    await Promise.all(children.map((c) => once(c, "message")));
    const results = children.map((c) => once(c, "message"));
    children.forEach((c) => c.send("go"));
    assert.deepEqual((await Promise.all(results)).map((r) => r[0]).sort(), [false, true]);
    const old = await acquire(`${path}.lock`, { timeoutMs: 1000, pollMs: 1 });
    await new Promise((r) => setTimeout(r, 10));
    assert.equal(await reclaim(`${path}.lock`, 1), true);
    const next = await acquire(`${path}.lock`, { timeoutMs: 1000, pollMs: 1 });
    await assert.rejects(old.assertOwned(), /CredentialStoreLeaseExpired/);
    assert.equal(await old.release(), false);
    await next.assertOwned();
    await next.release();
  } finally { children.forEach((c) => c.kill()); await rm(dir, { recursive: true, force: true }); }
});
test("live slow holder is not stolen and invalid maxHoldMs is rejected", async () => {
  const dir = await mkdtemp(join(tmpdir(), "r5-slow-"));
  try {
    const path = join(dir, "providers.json");
    const lock = await acquire(`${path}.lock`, { timeoutMs: 1000, pollMs: 1 });
    await assert.rejects(acquire(`${path}.lock`, { timeoutMs: 60, pollMs: 5 }), /CredentialStoreBusy/);
    await lock.assertOwned();
    await lock.release();
    for (const maxHoldMs of [0, -1, NaN, 0.5]) {
      assert.throws(() => new FileCredentials(path, new Redactor(), { timeoutMs: 1, pollMs: 1, maxHoldMs }));
    }
  } finally { await rm(dir, { recursive: true, force: true }); }
});

test("torn owner metadata waits for max hold age before recovery", async () => {
  const dir = await mkdtemp(join(tmpdir(), "r5-torn-"));
  const lock = join(dir, "store.lock");
  const generation = `${lock}/owner-abcdef`;
  try {
    await mkdir(generation, { recursive: true });
    await writeFile(`${generation}/owner.json`, "{");
    assert.equal(await reclaim(lock, 600000), false);
    await utimes(generation, new Date(0), new Date(0));
    assert.equal(await reclaim(lock, 600000), true);
  } finally { await rm(dir, { recursive: true, force: true }); }
});

import { mkdir, readdir, readFile, rename, rm, rmdir, writeFile, access, stat } from "node:fs/promises";
import { hostname } from "node:os";
import { randomUUID } from "node:crypto";
import { setTimeout as delay } from "node:timers/promises";
import { owner, processState, reclaimable, validOwner } from "./lock-owner.ts";

export interface LockOptions { timeoutMs: number; pollMs: number; maxHoldMs?: number }
export const DEFAULT_MAX_HOLD_MS = 600000;
// Publish a fully initialized, nonempty directory. Each generation has a unique
// child name: a delayed reaper can only rename the generation it inspected.
// Renaming that child to a unique tombstone is the atomic ownership operation.
// rmdir (never recursive removal of the live path) cannot delete a new owner.
export async function retire(lock: string, token: string): Promise<boolean> {
  const tomb = `${lock}.tomb.${randomUUID()}`;
  try { await rename(`${lock}/${token}`, tomb); }
  catch (e) { if ((e as NodeJS.ErrnoException).code === "ENOENT") return false; throw e; }
  await rmdir(lock).catch(() => undefined);
  await rm(tomb, { recursive: true, force: true });
  return true;
}
export async function reclaim(lock: string, maxHoldMs: number): Promise<boolean> {
  let names: string[];
  try { names = await readdir(lock); } catch { return false; }
  if (!names.length) { await rmdir(lock).catch(() => undefined); return false; }
  for (const token of names.filter((name) => /^owner-[\da-f-]+$/.test(name))) {
    let info: unknown;
    try { info = JSON.parse(await readFile(`${lock}/${token}/owner.json`, "utf8")); } catch { /* torn metadata */ }
    if (!validOwner(info)) {
      const modified = await stat(`${lock}/${token}`).then((s) => s.mtimeMs).catch(() => undefined);
      if (modified !== undefined && Date.now() - modified > maxHoldMs) return retire(lock, token);
      continue;
    }
    const state = info.hostname === hostname() ? await processState(info.pid) : { state: "unknown" as const };
    if (reclaimable(info, hostname(), state, Date.now(), maxHoldMs)) return retire(lock, token);
  }
  return false;
}
export async function acquire(lock: string, options: LockOptions, signal?: AbortSignal) {
  const token = `owner-${randomUUID()}`;
  const candidate = `${lock}.candidate.${randomUUID()}`;
  await mkdir(`${candidate}/${token}`, { recursive: true, mode: 0o700 });
  try {
    const info = await owner();
    const deadline = performance.now() + options.timeoutMs;
    while (true) {
      signal?.throwIfAborted();
      info.acquiredAt = Date.now();
      await writeFile(`${candidate}/${token}/owner.json`, JSON.stringify(info), { mode: 0o600 });
      try { await rename(candidate, lock); break; }
      catch (e) {
        if (!["EEXIST", "ENOTEMPTY", "EPERM", "EACCES"].includes((e as NodeJS.ErrnoException).code ?? "")) throw e;
        await reclaim(lock, options.maxHoldMs ?? DEFAULT_MAX_HOLD_MS);
        if (performance.now() >= deadline) throw new Error("CredentialStoreBusy");
        await delay(options.pollMs, undefined, { signal });
      }
    }
    return {
      async assertOwned() {
        try { await access(`${lock}/${token}/owner.json`); }
        catch { throw new Error("CredentialStoreLeaseExpired"); }
      },
      release: () => retire(lock, token),
    };
  } finally { await rm(candidate, { recursive: true, force: true }); }
}

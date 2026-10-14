import { mkdir, readFile, open, rename, rm } from "node:fs/promises";
import { dirname } from "node:path";
import { randomUUID } from "node:crypto";
import { acquire, DEFAULT_MAX_HOLD_MS, type LockOptions } from "./credential-lock.ts";
export type { LockOptions } from "./credential-lock.ts";
import type { Credential, CredentialStore, AuthOperationOptions } from "@earendil-works/pi-ai";
import { Redactor } from "./credentials.ts";

/** Shared generation-directory lock contract with Rust credential_lock.rs. */
export class FileCredentials implements CredentialStore {
  constructor(readonly path: string, private readonly redactor: Redactor,
    private readonly locks: LockOptions = { timeoutMs: 30000, pollMs: 25 }) {
    if (!path || !Number.isSafeInteger(locks.timeoutMs) || locks.timeoutMs <= 0
      || !Number.isSafeInteger(locks.maxHoldMs ?? DEFAULT_MAX_HOLD_MS) || (locks.maxHoldMs ?? DEFAULT_MAX_HOLD_MS) <= 0
      || !Number.isSafeInteger(locks.pollMs) || locks.pollMs <= 0) throw new Error("CredentialStoreConfigInvalid");
  }
  private async root(): Promise<Record<string, unknown> & { credentials?: Record<string, Credential> }> {
    try {
      const value = JSON.parse(await readFile(this.path, "utf8"));
      if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error();
      if (value.credentials && (typeof value.credentials !== "object" || Array.isArray(value.credentials))) throw new Error();
      for (const credential of Object.values(value.credentials ?? {})) this.register(credential as Credential);
      return value;
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === "ENOENT") return {};
      throw new Error("CredentialStoreReadFailed");
    }
  }
  private register(credential?: Credential): void {
    if (!credential) return;
    if (credential.type === "oauth") {
      this.redactor.add(credential.access);
      this.redactor.add(credential.refresh);
    } else if (credential.type === "api_key") {
      if (credential.key) this.redactor.add(credential.key);
      for (const v of Object.values(credential.env ?? {})) if (typeof v === "string") this.redactor.add(v);
    } else throw new Error("CredentialStoreInvalid");
  }
  private async locked<T>(work: (assertOwned: () => Promise<void>) => Promise<T>, options?: AuthOperationOptions): Promise<T> {
    await mkdir(dirname(this.path), { recursive: true });
    const lock = await acquire(`${this.path}.lock`, this.locks, options?.signal);
    try { options?.signal?.throwIfAborted(); return await work(lock.assertOwned); }
    finally { await lock.release(); }
  }
  private async save(root: object): Promise<void> {
    const temp = `${this.path}.${process.pid}.${randomUUID()}.tmp`;
    try {
      const file = await open(temp, "wx", 0o600);
      try { await file.writeFile(JSON.stringify(root, null, 2)); await file.sync(); }
      finally { await file.close(); }
      await rename(temp, this.path);
    } catch { throw new Error("CredentialStoreWriteFailed"); }
    finally { await rm(temp, { force: true }); }
  }
  async read(id: string, options?: AuthOperationOptions): Promise<Credential | undefined> {
    options?.signal?.throwIfAborted();
    const entries = (await this.root()).credentials;
    return entries && Object.hasOwn(entries, id) ? entries[id] : undefined;
  }
  async list(options?: AuthOperationOptions) {
    options?.signal?.throwIfAborted();
    return Object.entries((await this.root()).credentials ?? {}).map(([providerId, c]) => ({ providerId, type: c.type }));
  }
  async modify(id: string, fn: (current: Credential | undefined) => Promise<Credential | undefined>, options?: AuthOperationOptions) {
    return this.locked(async (assertOwned) => {
      const root = await this.root();
      const entries = root.credentials ??= Object.create(null) as Record<string, Credential>;
      const credential = await fn(Object.hasOwn(entries, id) ? entries[id] : undefined);
      if (credential !== undefined) {
        this.register(credential);
        Object.defineProperty(entries, id, { value: credential, enumerable: true, configurable: true, writable: true });
        await assertOwned();
        await this.save(root);
      }
      return entries[id];
    }, options);
  }
  async delete(id: string, options?: AuthOperationOptions): Promise<void> {
    await this.locked(async (assertOwned) => {
      const root = await this.root();
      if (root.credentials) delete root.credentials[id];
      await assertOwned();
      await this.save(root);
    }, options);
  }
}

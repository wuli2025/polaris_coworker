import type { Config } from "../config/schema.ts";
import type { SandboxResult } from "../protocol/schema.ts";
import { randomUUID } from "node:crypto";
import {
  createBashToolDefinition, createWriteToolDefinition, createEditToolDefinition,
  type ExtensionFactory,
} from "@earendil-works/pi-coding-agent";

export interface SandboxRequest { callId: string; operation: string; args: Record<string, unknown> }
const failure = (error: string): SandboxResult => ({ output: "", exitCode: null, signal: null, timedOut: false, error });
export class Sandbox {
  private readonly pending = new Map<string, (result: SandboxResult) => void>();
  private probe?: Promise<void>;
  private cached?: { until: number; error?: Error };
  constructor(public enabled: boolean, private readonly cwd: string, private readonly config: Config["sandbox"],
    private readonly emit: (request: SandboxRequest) => void) {}
  respond(callId: string, result: SandboxResult): boolean {
    const finish = this.pending.get(callId);
    if (!finish) return false;
    finish(result);
    return true;
  }
  cancel(): void {
    for (const finish of this.pending.values()) finish(failure("SANDBOX_ABORTED"));
  }
  async available(): Promise<void> {
    if (this.cached && Date.now() < this.cached.until) {
      if (this.cached.error) throw this.cached.error;
      return;
    }
    if (!this.probe) {
      this.probe = this.exchange("probe", {}, undefined, this.config.probeTimeoutMs).then((result) => {
        if (result.exitCode !== 0 || result.timedOut || result.signal) throw new Error("Probe failed");
        this.cached = { until: Date.now() + this.config.probeCacheMs };
      }).catch((cause: unknown) => {
        const error = new Error(`SANDBOX_UNAVAILABLE: ${String(cause)}`);
        this.cached = { until: Date.now() + this.config.probeCacheMs, error };
        throw error;
      }).finally(() => { this.probe = undefined; });
    }
    return this.probe;
  }
  async request(operation: string, args: Record<string, unknown>, signal?: AbortSignal): Promise<SandboxResult> {
    await this.available();
    return this.exchange(operation, args, signal, this.config.timeoutMs);
  }
  private exchange(operation: string, args: Record<string, unknown>, signal: AbortSignal | undefined,
    timeoutMs: number): Promise<SandboxResult> {
    return new Promise((resolve, reject) => {
      if (signal?.aborted) return reject(new Error("SANDBOX_ABORTED"));
      const callId = randomUUID();
      const abort = () => finish(failure("SANDBOX_ABORTED"));
      const timer = setTimeout(() => finish(failure("SANDBOX_TIMEOUT: no host execution result")), timeoutMs);
      const finish = (result: SandboxResult) => {
        clearTimeout(timer);
        signal?.removeEventListener("abort", abort);
        this.pending.delete(callId);
        if (result.error) reject(new Error(result.error)); else resolve(result);
      };
      this.pending.set(callId, finish);
      signal?.addEventListener("abort", abort, { once: true });
      try { this.emit({ callId, operation, args: { ...args, cwd: this.cwd, ...this.config, timeoutMs } }); }
      catch { finish(failure("SANDBOX_UNAVAILABLE")); }
    });
  }
  factory(): ExtensionFactory {
    return (pi) => {
      // Probe even before read-only/custom tools in a sandbox-enabled session.
      pi.on("tool_call", async () => {
        if (!this.enabled) return;
        try { await this.available(); }
        catch (error) { return { block: true, reason: String(error) }; }
      });
      const file = async (operation: string, args: Record<string, unknown>) => {
        const result = await this.request(operation, args);
        if (result.timedOut || result.signal || result.exitCode !== 0) throw new Error("SANDBOX_EXEC_FAILED");
        return result.output;
      };
      const writeFile = async (path: string, content: string) => { await file("write", { path, content }); };
      const remoteWrite = createWriteToolDefinition(this.cwd, { operations: {
        writeFile, mkdir: async (path) => { await file("mkdir", { path }); },
      } });
      const remoteEdit = createEditToolDefinition(this.cwd, { operations: {
        writeFile, readFile: async (path) => Buffer.from(await file("read", { path }), "base64"),
        access: async (path) => { await file("access", { path }); },
      } });
      const remoteBash = createBashToolDefinition(this.cwd, { exposeSessionEnvironment: false, operations: {
        exec: async (command, _cwd, options) => {
          const result = await this.request("bash", { command, timeout: options.timeout }, options.signal);
          options.onData(Buffer.from(result.output));
          if (result.timedOut) options.onData(Buffer.from("\nSANDBOX_TIMEOUT\n"));
          if (result.signal) options.onData(Buffer.from(`\nSANDBOX_SIGNAL: ${result.signal}\n`));
          return result;
        },
      } });
      const write = createWriteToolDefinition(this.cwd);
      const edit = createEditToolDefinition(this.cwd);
      const bash = createBashToolDefinition(this.cwd);
      pi.registerTool({ ...write, execute: (...args) => (this.enabled ? remoteWrite : write).execute(...args) });
      pi.registerTool({ ...edit, execute: (...args) => (this.enabled ? remoteEdit : edit).execute(...args) });
      pi.registerTool({ ...bash, execute: (...args) => (this.enabled ? remoteBash : bash).execute(...args) });
    };
  }
}

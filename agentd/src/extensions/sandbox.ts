import type { Config } from "../config/schema.ts";
import { randomUUID } from "node:crypto";
import {
  createBashToolDefinition, createWriteToolDefinition, createEditToolDefinition,
  type ExtensionFactory,
} from "@earendil-works/pi-coding-agent";

export interface SandboxRequest { callId: string; operation: string; args: Record<string, unknown> }
export class Sandbox {
  private readonly pending = new Map<string, (output: string, error?: string) => void>();
  constructor(public enabled: boolean, private readonly cwd: string, private readonly config: Config["sandbox"],
    private readonly emit: (request: SandboxRequest) => void) {}
  respond(callId: string, output: string, error?: string): boolean {
    const finish = this.pending.get(callId);
    if (!finish) return false;
    finish(output, error);
    return true;
  }
  cancel(): void { for (const finish of this.pending.values()) finish("", "SANDBOX_ABORTED"); }
  request(operation: string, args: Record<string, unknown>, signal?: AbortSignal): Promise<string> {
    return new Promise((resolve, reject) => {
      if (signal?.aborted) return reject(new Error("SANDBOX_ABORTED"));
      const callId = randomUUID();
      const abort = () => finish("", "SANDBOX_ABORTED");
      const timer = setTimeout(() => finish("", "SANDBOX_TIMEOUT: no host execution result"), this.config.timeoutMs);
      const finish = (output: string, error?: string) => {
        clearTimeout(timer);
        signal?.removeEventListener("abort", abort);
        this.pending.delete(callId);
        if (error) reject(new Error(error)); else resolve(output);
      };
      this.pending.set(callId, finish);
      signal?.addEventListener("abort", abort, { once: true });
      try { this.emit({ callId, operation, args: { ...args, cwd: this.cwd, ...this.config } }); }
      catch { finish("", "SANDBOX_UNAVAILABLE"); }
    });
  }
  factory(): ExtensionFactory {
    return (pi) => {
      const writeFile = async (path: string, content: string) => { await this.request("write", { path, content }); };
      const remoteWrite = createWriteToolDefinition(this.cwd, { operations: {
        writeFile, mkdir: async (path) => { await this.request("mkdir", { path }); },
      } });
      const remoteEdit = createEditToolDefinition(this.cwd, { operations: {
        writeFile,
        readFile: async (path) => Buffer.from(await this.request("read", { path }), "base64"),
        access: async (path) => { await this.request("access", { path }); },
      } });
      const remoteBash = createBashToolDefinition(this.cwd, { exposeSessionEnvironment: false, operations: {
        exec: async (command, _cwd, options) => {
          const result = await this.request("bash", { command, timeout: options.timeout }, options.signal);
          options.onData(Buffer.from(result));
          return { exitCode: 0 };
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

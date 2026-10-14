import { homedir } from "node:os";
import { fileURLToPath } from "node:url";
import { realpath, lstat } from "node:fs/promises";
import { dirname, resolve, relative, isAbsolute, basename, sep } from "node:path";
import type { ExtensionFactory } from "@earendil-works/pi-coding-agent";
import type { PermissionMode } from "../protocol/schema.ts";

export type Decision = "allow" | "deny" | "allowAlways";
export interface Approval { callId: string; tool: string; summary: string; expiresAt: number }
export interface Resolution { callId: string; reason: "responded" | "expired" | "cancelled" }
const readers = new Set(["read", "grep", "find", "ls", "web_fetch", "web_search", "subagent"]);

// Resolve the closest existing ancestor, including symlinks, for new files.
async function canonical(path: string): Promise<string> {
  try { return await realpath(path); }
  catch (error) {
    if ((error as NodeJS.ErrnoException).code !== "ENOENT" || dirname(path) === path) throw error;
    // A dangling symlink exists even though realpath reports ENOENT. Never treat it as a new component.
    try { await lstat(path); throw new Error("Unresolved existing path"); }
    catch (missing) { if ((missing as NodeJS.ErrnoException).code !== "ENOENT") throw missing; }
    return resolve(await canonical(dirname(path)), basename(path));
  }
}
// Match the pinned Pi tool path syntax before the containment check. Pi normalizes
// these forms at execution, so checking their unexpanded spelling would be unsafe.
export function toolPath(input: string): string {
  let path = input.replace(/[\u00A0\u2000-\u200A\u202F\u205F\u3000]/g, " ");
  if (path.startsWith("@")) path = path.slice(1);
  if (process.platform === "win32" && path.startsWith("/") && !path.startsWith("//") && !path.includes("\\")) {
    const drive = path.match(/^\/(?:mnt\/|cygdrive\/)?([a-z])(?:\/(.*))?$/i);
    if (drive) path = `${drive[1].toUpperCase()}:\\${(drive[2] ?? "").replaceAll("/", "\\")}`;
  }
  if (path === "~") return homedir();
  if (path.startsWith("~/") || (process.platform === "win32" && path.startsWith("~\\"))) return `${homedir()}${sep}${path.slice(2)}`;
  return path.startsWith("file://") ? fileURLToPath(path) : path;
}
export async function withinWorkspace(cwd: string, path: unknown): Promise<boolean> {
  if (typeof path !== "string" || !path) return false;
  try {
    // Reject parent segments before URL conversion or any lexical normalization.
    if (path.split(/[\\/]/).includes("..")) return false;
    const expanded = toolPath(path);
    if (expanded.split(/[\\/]/).includes("..")) return false;
    // On POSIX a backslash is a literal filename character; do not reinterpret a foreign path.
    if (process.platform !== "win32" && (expanded.includes("\\") || /^[a-z]:/i.test(expanded))) return false;
    const absolute = isAbsolute(expanded) ? expanded : `${cwd}${sep}${expanded}`;
    const rel = relative(await realpath(cwd), await canonical(absolute));
    return !isAbsolute(rel) && rel !== ".." && !rel.startsWith("../") && !rel.startsWith("..\\");
  } catch { return false; }
}
export class Permissions {
  private readonly remembered = new Set<string>();
  private readonly pending = new Map<string, (decision: Decision, reason?: Resolution["reason"]) => void>();
  constructor(public mode: PermissionMode, private readonly cwd: string,
    private readonly timeoutMs: number, private readonly summaryChars: number,
    private readonly emit: (approval: Approval) => void,
    private readonly resolved: (resolution: Resolution) => void = () => {}) {}
  configure(mode: PermissionMode): void {
    this.cancel();
    this.remembered.clear();
    this.mode = mode;
  }
  respond(callId: string, decision: Decision): boolean {
    const resolve = this.pending.get(callId);
    if (!resolve) return false;
    resolve(decision);
    return true;
  }
  cancel(): void { for (const resolve of this.pending.values()) resolve("deny", "cancelled"); }
  async check(callId: string, tool: string, args: Record<string, unknown>): Promise<boolean> {
    if (this.mode === "auto-all" || readers.has(tool)) return true;
    if (this.mode === "deny") return false;
    const inside = ["write", "edit"].includes(tool) && await withinWorkspace(this.cwd, args.path);
    if (this.mode === "auto-current" && inside) return true;
    // allowAlways is scoped to this exact tool and arguments, in this session.
    const key = JSON.stringify([tool, args]);
    if (this.remembered.has(key)) return true;
    if (this.pending.has(callId)) return false;
    const decision = await new Promise<Decision>((resolve) => {
      const timer = setTimeout(() => finish("deny", "expired"), this.timeoutMs);
      const finish = (decision: Decision, reason: Resolution["reason"] = "responded") => {
        if (!this.pending.has(callId)) return;
        clearTimeout(timer);
        this.pending.delete(callId);
        try { this.resolved({ callId, reason }); } finally { resolve(decision); }
      };
      this.pending.set(callId, finish);
      try { this.emit({ callId, tool, summary: JSON.stringify(args).slice(0, this.summaryChars), expiresAt: Date.now() + this.timeoutMs }); }
      catch { finish("deny"); }
    });
    if (decision === "allowAlways") this.remembered.add(key);
    return decision !== "deny";
  }
  factory(): ExtensionFactory {
    return (pi) => {
      pi.on("tool_call", async (event) => {
        if (!await this.check(event.toolCallId, event.toolName, event.input)) {
          return { block: true, reason: "PERMISSION_DENIED: host rejected the operation, approval expired, or the session is read-only." };
        }
      });
    };
  }
}

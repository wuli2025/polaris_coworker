import { readFile } from "node:fs/promises";
import { hostname } from "node:os";
import { execFile } from "node:child_process";
import { promisify } from "node:util";

export interface Owner { hostname: string; pid: number; processStart: string; acquiredAt: number }
export type ProcessState = { state: "alive"; start: string } | { state: "dead" | "unknown" };
const exec = promisify(execFile);
export async function processState(pid: number): Promise<ProcessState> {
  if (process.platform === "linux") {
    try {
      const stat = await readFile(`/proc/${pid}/stat`, "utf8");
      const fields = stat.slice(stat.lastIndexOf(")") + 2).split(" ");
      if (fields[0] === "Z" || fields[0] === "X") return { state: "dead" };
      const boot = (await readFile("/proc/sys/kernel/random/boot_id", "utf8")).trim();
      return { state: "alive", start: `${boot}:${fields[19]}` };
    } catch (e) { return { state: (e as NodeJS.ErrnoException).code === "ENOENT" ? "dead" : "unknown" }; }
  }
  try {
    const { stdout } = process.platform === "win32"
      ? await exec("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command",
        `try { $p = Get-Process -Id ${pid} -ErrorAction Stop; $p.StartTime.ToUniversalTime().Ticks } catch { if ($_.FullyQualifiedErrorId -like 'NoProcessFoundForGivenId*') { 'dead' } else { exit 2 } }`])
      : await exec("ps", ["-p", String(pid), "-o", "lstart="], { env: { ...process.env, LC_ALL: "C" } });
    return stdout.trim() && stdout.trim() !== "dead" ? { state: "alive", start: stdout.trim() } : { state: "dead" };
  } catch (error) {
    const e = error as { code?: number; stdout?: string };
    return { state: process.platform !== "win32" && e.code === 1 && e.stdout === "" ? "dead" : "unknown" };
  }
}
export async function owner(): Promise<Owner> {
  const state = await processState(process.pid);
  if (state.state !== "alive") throw new Error("CredentialStoreProcessIdentityFailed");
  return { hostname: hostname(), pid: process.pid, processStart: state.start, acquiredAt: Date.now() };
}
export function reclaimable(info: Owner, host: string, process: ProcessState, now: number, maxHoldMs: number): boolean {
  return now - info.acquiredAt > maxHoldMs || (info.hostname === host
    && (process.state === "dead" || (process.state === "alive" && process.start !== info.processStart)));
}
export function validOwner(value: unknown): value is Owner {
  const v = value as Partial<Owner> | null;
  return !!v && typeof v.hostname === "string" && !!v.hostname && Number.isSafeInteger(v.pid) && v.pid! > 0 && v.pid! <= 4294967295
    && typeof v.processStart === "string" && !!v.processStart && Number.isSafeInteger(v.acquiredAt) && v.acquiredAt! >= 0;
}

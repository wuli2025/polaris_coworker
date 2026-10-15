import { EventEmitter } from "node:events";

export function waitBudget(value = process.env.POLARIS_TEST_WAIT_MS): number {
  const ms = value === undefined ? 30_000 : Number(value);
  if (!Number.isSafeInteger(ms) || ms < 1000 || ms > 300_000) {
    throw new Error("POLARIS_TEST_WAIT_MS must be an integer in [1000, 300000]");
  }
  return ms;
}
// SDK imports compete with every test worker. This is a failure ceiling, never a sleep.
export const WAIT_MS = waitBudget();
export async function bounded<T>(promise: Promise<T>, label: string, ms = WAIT_MS): Promise<T> {
  let timer: NodeJS.Timeout | undefined;
  try {
    return await Promise.race([promise, new Promise<never>((_, reject) => {
      timer = setTimeout(() => reject(new Error(`Timed out: ${label} (${ms}ms)`)), ms);
    })]);
  } finally { clearTimeout(timer); }
}

// Level-triggered observation: catches events both before and after subscription.
export class Changes {
  private readonly events = new EventEmitter();
  private error?: Error;
  notify(): void { this.events.emit("change"); }
  close(error: Error): void { this.error = error; this.notify(); }
  async wait(predicate: () => boolean, label: string, ms = WAIT_MS): Promise<void> {
    let check!: () => void;
    const pending = new Promise<void>((resolve, reject) => {
      check = () => {
        try {
          if (predicate()) resolve();
          else if (this.error) reject(this.error);
        } catch (error) { reject(error); }
      };
      this.events.on("change", check);
      check();
    });
    try { await bounded(pending, label, ms); }
    finally { this.events.off("change", check); }
  }
}

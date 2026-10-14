import type { Config } from "./config/schema.ts";
import type { Redactor } from "./models/credentials.ts";
const levels = { debug: 0, info: 1, warn: 2, error: 3, silent: 4 };
export class Logger {
  constructor(
    private readonly level: () => Config["logLevel"],
    private readonly redactor: Redactor,
    private readonly write: (line: string) => void,
  ) {}
  log(level: Exclude<Config["logLevel"], "silent">, message: string,
    context: { requestId?: string | number; sessionId?: string; code?: string; paths?: string[] } = {}): void {
    if (levels[level] < levels[this.level()]) return;
    this.write(JSON.stringify(this.redactor.clean({ time: new Date().toISOString(), level, message, ...context })) + "\n");
  }
}

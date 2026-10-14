import type { Writable } from "node:stream";
import { DaemonError } from "./errors.ts";
import type { Redactor } from "../models/credentials.ts";

/** Buffer bytes, not JS characters; discard an oversized frame through its newline. */
export class JsonlReader {
  private chunks: Buffer[] = [];
  private size = 0;
  private dropping = false;
  constructor(
    private readonly limit: () => number,
    private readonly receive: (value: unknown) => void,
    private readonly fail: (error: DaemonError) => void,
  ) {}
  push(chunk: Buffer): void {
    let offset = 0;
    while (offset < chunk.length) {
      const end = chunk.indexOf(10, offset);
      const part = chunk.subarray(offset, end < 0 ? chunk.length : end);
      if (!this.dropping) {
        this.size += part.length;
        if (this.size > this.limit()) {
          this.chunks = [];
          this.dropping = true;
          this.fail(new DaemonError("LINE_TOO_LONG", "Input line exceeds maxLineBytes"));
        } else this.chunks.push(part);
      }
      if (end < 0) break;
      this.finish();
      offset = end + 1;
    }
  }
  end(): void {
    if (this.size || this.dropping) this.finish();
  }
  private finish(): void {
    if (!this.dropping && this.size) {
      try {
        const line = new TextDecoder("utf-8", { fatal: true }).decode(Buffer.concat(this.chunks));
        this.receive(JSON.parse(line));
      } catch {
        this.fail(new DaemonError("INVALID_JSON", "Input must be UTF-8 JSON"));
      }
    }
    this.chunks = [];
    this.size = 0;
    this.dropping = false;
  }
}

export class JsonlWriter {
  private tail = Promise.resolve();
  private failure: unknown;
  constructor(private readonly output: Writable, private readonly redactor: Redactor) {
    output.on("error", (error) => { this.failure = error; });
  }
  send(value: unknown): void {
    const line = JSON.stringify(this.redactor.clean(value)) + "\n";
    this.tail = this.tail.then(() => new Promise<void>((resolve, reject) => {
      if (this.failure) return reject(this.failure);
      this.output.write(line, (error) => error ? reject(error) : resolve());
    })).catch((error) => { this.failure = error; });
  }
  async flush(): Promise<void> {
    await this.tail;
    if (this.failure) throw this.failure;
  }
}

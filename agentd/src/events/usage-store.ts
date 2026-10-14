import { mkdirSync, openSync, closeSync, writeFileSync, fsyncSync, renameSync, rmSync } from "node:fs";
import { join } from "node:path";
import { createHash, randomUUID } from "node:crypto";
import type { AssistantMessage } from "@earendil-works/pi-ai";
import { Value } from "typebox/value";
import { UsageSchema } from "../protocol/events.ts";

/** One atomic record per provider request; no prompts, headers, or credentials. */
export function recordUsage(directory: string, sessionId: string, turnId: string, ordinal: number,
  message: AssistantMessage): void {
  if (!directory) return;
  if (!Value.Check(UsageSchema, message.usage) || !Number.isFinite(message.timestamp)) throw new Error("UsageInvalid");
  const date = new Date(message.timestamp).toISOString().slice(0, 10);
  const requestId = message.responseId || `${sessionId}:${turnId}:${ordinal}`;
  const id = createHash("sha256").update(JSON.stringify([date, message.provider, requestId])).digest("hex");
  const dir = join(directory, date);
  mkdirSync(dir, { recursive: true });
  const path = join(dir, `${id}.json`);
  const temp = `${path}.${randomUUID()}.tmp`;
  const row = { version: 1, sessionId, turnId, requestId, provider: message.provider,
    model: message.model, timestamp: message.timestamp, date, usage: message.usage,
    // Only Anthropic's native message id can identify a copied legacy CLI row.
    legacyMessageId: message.api === "anthropic-messages" ? message.responseId : undefined };
  try {
    const fd = openSync(temp, "wx", 0o600);
    try { writeFileSync(fd, JSON.stringify(row)); fsyncSync(fd); } finally { closeSync(fd); }
    renameSync(temp, path);
  } finally { rmSync(temp, { force: true }); }
}

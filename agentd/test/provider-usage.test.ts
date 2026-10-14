import assert from "node:assert/strict";
import { test } from "node:test";
import { mkdtempSync, readdirSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fauxAssistantMessage } from "@earendil-works/pi-ai";
import { recordUsage } from "../src/events/usage-store.ts";

test("usage replay deduplicates a request; distinct requests/days remain separate", () => {
  const dir = mkdtempSync(join(tmpdir(), "p4-usage-"));
  const message = { ...fauxAssistantMessage("content must not be persisted"),
    api: "anthropic-messages", provider: "anthropic", model: "test-model",
    timestamp: Date.UTC(2026, 9, 14), responseId: "msg-1" };
  try {
    recordUsage(dir, "session-a", "turn-a", 0, message);
    recordUsage(dir, "session-a", "turn-a", 0, message);
    const files = readdirSync(join(dir, "2026-10-14"));
    assert.equal(files.length, 1);
    const row = JSON.parse(readFileSync(join(dir, "2026-10-14", files[0]), "utf8"));
    assert.equal(row.legacyMessageId, "msg-1");
    assert.equal(row.sessionId, "session-a");
    assert.deepEqual(row.usage, message.usage);
    assert.ok(!JSON.stringify(row).includes("content must not"));
    recordUsage(dir, "session-a", "turn-a", 1, { ...message, responseId: "msg-2" });
    recordUsage(dir, "session-a", "turn-b", 0, { ...message, timestamp: message.timestamp + 86400000 });
    assert.equal(readdirSync(join(dir, "2026-10-14")).length, 2);
    assert.equal(readdirSync(join(dir, "2026-10-15")).length, 1);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

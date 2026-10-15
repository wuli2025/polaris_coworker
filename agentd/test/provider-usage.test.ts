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
  const runtime = { getModel: () => undefined };
  try {
    recordUsage(dir, "session-a", "turn-a", 0, message, runtime);
    recordUsage(dir, "session-a", "turn-a", 0, message, runtime);
    const files = readdirSync(join(dir, "2026-10-14"));
    assert.equal(files.length, 1);
    const row = JSON.parse(readFileSync(join(dir, "2026-10-14", files[0]), "utf8"));
    assert.equal(row.legacyMessageId, "msg-1");
    assert.equal(row.sessionId, "session-a");
    assert.equal(row.priceAvailable, false);
    assert.deepEqual(row.usage, message.usage);
    assert.ok(!JSON.stringify(row).includes("content must not"));
    recordUsage(dir, "session-a", "turn-a", 1, { ...message, responseId: "msg-2" }, runtime);
    recordUsage(dir, "session-a", "turn-b", 0, { ...message, timestamp: message.timestamp + 86400000 }, runtime);
    assert.equal(readdirSync(join(dir, "2026-10-14")).length, 2);
    assert.equal(readdirSync(join(dir, "2026-10-15")).length, 1);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test("usage persistence failure is logged and counted beyond its turn", async () => {
  const { harness, result, until } = await import("./helpers.ts");
  const { writeFileSync } = await import("node:fs");
  const h = await harness([fauxAssistantMessage("first"), fauxAssistantMessage("second")]);
  try {
    const blocked = join(h.dir, "blocked");
    writeFileSync(blocked, "file prevents mkdir");
    result(await h.request("config.apply", { config: { usageDir: blocked } }));
    const { sessionId } = result<{ sessionId: string }>(await h.request("session.open"));
    for (let turn = 1; turn <= 2; turn++) {
      result(await h.request("prompt", { sessionId, text: "test" }));
      await until(() => h.events.filter((e) => e.event === "turn.completed").length === turn);
      assert.equal(result<{ usagePersistFailures: number }>(await h.request("ping")).usagePersistFailures, turn);
    }
    const warnings = h.logs.map((line) => JSON.parse(line)).filter((l) => l.code === "USAGE_PERSIST_FAILED");
    assert.equal(warnings.length, 2);
    assert.equal(warnings[1].usagePersistFailures, 2);
    assert.ok(!h.logs.join("").includes(blocked));
  } finally { await h.close(); }
});

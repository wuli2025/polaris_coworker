import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { Value } from "typebox/value";
import { JsonlReader } from "../src/protocol/jsonl.ts";
import { EventSchema, type ProtocolEvent } from "../src/protocol/events.ts";
import { ResponseSchema, type Response } from "../src/protocol/schema.ts";
import { until, result } from "./helpers.ts";

test("main.ts subprocess speaks JSONL through a real SDK tool round and shuts down via EOF/SIGTERM", { timeout: 15000 }, async () => {
  for (const signal of [false, true]) {
    const dir = mkdtempSync(join(tmpdir(), "agentd-e2e-"));
    const config = join(dir, "config.json");
    writeFileSync(join(dir, "sample.txt"), "tool-result-marker");
    writeFileSync(config, JSON.stringify({
      sessionDir: join(dir, "sessions"), tools: ["read"], logLevel: "debug", maxLineBytes: 4096,
      defaultModel: { provider: "e2e-faux", id: "e2e-model" },
      providers: [{ id: "e2e-faux", api: "e2e-faux-api", baseUrl: "https://unused.invalid",
        models: [{ id: "e2e-model", name: "Faux E2E", reasoning: false, input: ["text"],
          contextWindow: 100000, maxTokens: 1000, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 } }] }],
    }));
    const child = spawn(process.execPath, ["--import", "tsx", "--import", "./test/fixtures/preload.ts",
      "src/main.ts", "--config", config], { cwd: fileURLToPath(new URL("..", import.meta.url)),
      stdio: ["pipe", "pipe", "pipe"], env: { ...process.env, NODE_TEST_CONTEXT: undefined, PI_OFFLINE: "1" } });
    const exited = once(child, "exit");
    const records: (Response | ProtocolEvent)[] = [];
    const parseErrors: string[] = [];
    let stderr = "";
    const reader = new JsonlReader(() => 1000000, (value) => records.push(value as Response | ProtocolEvent),
      (error) => parseErrors.push(error.code));
    child.stdout.on("data", (chunk: Buffer) => reader.push(chunk));
    child.stderr.on("data", (chunk: Buffer) => { stderr += chunk.toString(); });
    let id = 0;
    const request = async (method: string, params: object = {}) => {
      const requestId = ++id;
      child.stdin.write(JSON.stringify({ id: requestId, method, params }) + "\n");
      await until(() => records.some((r) => r.type === "response" && r.id === requestId))
        .catch((error) => { throw new Error(`${error}; exit=${child.exitCode}; stderr=${stderr}`); });
      return records.find((r): r is Response => r.type === "response" && r.id === requestId)!;
    };
    try {
      assert.equal(result<{ status: string }>(await request("ping")).status, "ok");
      child.stdin.write("x".repeat(5000) + "\nnot-json\n");
      await until(() => records.filter((r) => r.type === "response" && !r.ok).length === 2);
      assert.deepEqual(records.filter((r) => r.type === "response" && !r.ok).map((r) => r.error.code),
        ["LINE_TOO_LONG", "INVALID_JSON"]);
      const key = "never-log-this-secret-784621";
      assert.ok((await request("config.apply", { credentials: { "e2e-faux": key } })).ok);
      const sessionId = result<{ sessionId: string }>(await request("session.open", { cwd: dir })).sessionId;
      const turnId = result<{ turnId: string }>(await request("prompt", { sessionId, text: "read sample.txt" })).turnId;
      await until(() => records.some((r) => r.type === "event" && r.event === "turn.completed" && r.turnId === turnId));
      const events = records.filter((r): r is ProtocolEvent => r.type === "event" && r.turnId === turnId);
      assert.ok(events.some((e) => e.event === "tool.completed" && !e.isError && JSON.stringify(e.result).includes("tool-result-marker")));
      assert.equal(events.filter((e) => e.event === "text.delta").map((e) => e.text).join(""), "tool round trip complete");
      assert.ok(events.every((e) => e.sessionId === sessionId));
      assert.ok(events.some((e) => e.event === "turn.completed" && e.status === "completed"));
      // Shutdown during active model work must abort and flush the terminal event.
      const second = result<{ turnId: string }>(await request("prompt", { sessionId, text: "wait" })).turnId;
      await until(() => records.some((r) => r.type === "event" && r.event === "turn.started" && r.turnId === second));
      if (signal) child.kill("SIGTERM");
      else child.stdin.end();
      const [code, exitSignal] = await exited;
      assert.equal(code, 0, stderr);
      assert.equal(exitSignal, null);
      assert.ok(records.some((r) => r.type === "event" && r.event === "turn.completed" && r.turnId === second && r.status === "aborted"));
      assert.deepEqual(parseErrors, []);
      for (const record of records) {
        assert.ok(Value.Check(record.type === "event" ? EventSchema : ResponseSchema, record), JSON.stringify(record));
      }
      assert.ok(!stderr.includes(key));
      assert.ok(!JSON.stringify(records).includes(key));
      assert.ok(stderr.trim().split("\n").every((line) => JSON.parse(line).level));
    } finally {
      if (child.exitCode === null) child.kill("SIGKILL");
      await exited;
      rmSync(dir, { recursive: true, force: true });
    }
  }
});

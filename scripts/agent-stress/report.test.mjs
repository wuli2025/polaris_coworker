import { mkdtemp, readFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";

import { describe, expect, it } from "vitest";

import { writeReport } from "./report.mjs";

describe("stress report", () => {
  it("writes restart-safe JSONL and a redacted aggregate markdown report", async () => {
    const outputDir = await mkdtemp(path.join(tmpdir(), "polaris-report-"));
    const secret = "exact-secret-value";
    const records = [
      {
        runId: "run-1",
        providerId: "stress-kimi-k3",
        scenarioId: "browser-test",
        domain: "browser",
        startedAt: "2026-08-21T00:00:00.000Z",
        finishedAt: "2026-08-21T00:00:01.000Z",
        metrics: { durationMs: 1000, toolCalls: 1 },
        validation: {
          ok: false,
          checks: [{ name: "stream-errors", ok: false, detail: `failed ${secret}` }],
        },
        classification: "provider",
        retryCount: 1,
        manifests: [],
        error: "sk-report_1234567890abcdef",
      },
      {
        runId: "run-1",
        providerId: "stress-minimax-m3",
        scenarioId: "code-test",
        domain: "code",
        startedAt: "2026-08-21T00:00:02.000Z",
        finishedAt: "2026-08-21T00:00:03.000Z",
        metrics: { durationMs: 1000, toolCalls: 2 },
        validation: { ok: true, checks: [] },
        classification: "pass",
        retryCount: 0,
        manifests: [],
      },
    ];

    const paths = await writeReport(records, outputDir, { secrets: [secret] });
    const jsonl = await readFile(paths.jsonlPath, "utf8");
    const markdown = await readFile(paths.markdownPath, "utf8");

    expect(jsonl.trim().split("\n")).toHaveLength(2);
    expect(jsonl).not.toContain(secret);
    expect(jsonl).not.toContain("sk-report_1234567890abcdef");
    expect(markdown).toContain("1/2");
    expect(markdown).toContain("provider");
    expect(markdown).not.toContain(secret);
  });
});

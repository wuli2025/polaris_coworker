import { mkdir, rename, writeFile } from "node:fs/promises";
import path from "node:path";

import { redact } from "./redact.mjs";

async function atomicWrite(filePath, content) {
  const temporary = `${filePath}.tmp-${process.pid}`;
  await writeFile(temporary, content, "utf8");
  await rename(temporary, filePath);
}

function countBy(records, key) {
  const counts = new Map();
  for (const record of records) {
    const value = record[key] || "unknown";
    const current = counts.get(value) || { total: 0, passed: 0 };
    current.total += 1;
    if (record.validation?.ok) current.passed += 1;
    counts.set(value, current);
  }
  return [...counts.entries()].sort(([left], [right]) => left.localeCompare(right));
}

function markdownReport(records) {
  const passed = records.filter((record) => record.validation?.ok).length;
  const lines = [
    "# Polaris Agent Capability Stress Report",
    "",
    `Overall: ${passed}/${records.length} passed.`,
    "",
    "## Providers",
    "",
    "| Provider | Passed | Total |",
    "|---|---:|---:|",
    ...countBy(records, "providerId").map(
      ([provider, count]) => `| ${provider} | ${count.passed} | ${count.total} |`,
    ),
    "",
    "## Domains",
    "",
    "| Domain | Passed | Total |",
    "|---|---:|---:|",
    ...countBy(records, "domain").map(
      ([domain, count]) => `| ${domain} | ${count.passed} | ${count.total} |`,
    ),
    "",
    "## Classifications",
    "",
    "| Classification | Count |",
    "|---|---:|",
    ...[...records.reduce((counts, record) => {
      const key = record.classification || "unknown";
      counts.set(key, (counts.get(key) || 0) + 1);
      return counts;
    }, new Map())]
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([classification, count]) => `| ${classification} | ${count} |`),
    "",
    "## Runs",
    "",
    "| Provider | Scenario | Domain | Result | Classification | Duration ms | Retries |",
    "|---|---|---|---|---|---:|---:|",
    ...records.map(
      (record) =>
        `| ${record.providerId} | ${record.scenarioId} | ${record.domain} | ${record.validation?.ok ? "PASS" : "FAIL"} | ${record.classification} | ${record.metrics?.durationMs ?? ""} | ${record.retryCount ?? 0} |`,
    ),
    "",
  ];
  return lines.join("\n");
}

export async function writeReport(records, outputDir, { secrets = [] } = {}) {
  await mkdir(outputDir, { recursive: true });
  const safeRecords = redact(records, secrets);
  const jsonlPath = path.join(outputDir, "events.jsonl");
  const markdownPath = path.join(outputDir, "report.md");
  const jsonl = safeRecords.map((record) => JSON.stringify(record)).join("\n") +
    (safeRecords.length > 0 ? "\n" : "");
  await atomicWrite(jsonlPath, jsonl);
  await atomicWrite(markdownPath, markdownReport(safeRecords));
  return { jsonlPath, markdownPath };
}

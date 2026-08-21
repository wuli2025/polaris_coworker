#!/usr/bin/env node

import { readFile } from "node:fs/promises";
import path from "node:path";
import { pathToFileURL } from "node:url";

import { PolarisClient } from "./client.mjs";
import { startFixtureServer } from "./fixture-server.mjs";
import { configureProviders, parseHarnessArgs, runScenario } from "./orchestrator.mjs";
import { writeReport } from "./report.mjs";
import { SCENARIOS } from "./scenarios.mjs";

const REPO_ROOT = path.resolve(import.meta.dirname, "../..");
const RUNTIME_ROOT = path.join(REPO_ROOT, "_agent_stress");
const OUTPUT_DIR = path.join(RUNTIME_ROOT, "results", "latest");
const WORK_ROOT = path.join(RUNTIME_ROOT, "work");
const PREFLIGHT_IDS = [
  "browser-catalog-first-page",
  "code-discount-percentage",
  "ppt-12-executive",
];
const CONCURRENCY_IDS = [
  "browser-multipage-local",
  "browser-dynamic-aggregate",
  "browser-layout-change",
  "code-invalid-quantity",
  "code-immutable-sort",
  "code-next-order-id",
  "ppt-12-bilingual",
  "ppt-30-training",
];
const RECOVERY_IDS = ["browser-slow-response", "code-quantity-summary", "ppt-60-recovery"];

function createRunId() {
  return new Date().toISOString().replace(/[:.]/g, "-");
}

function selectByIds(ids) {
  const selected = ids.map((id) => SCENARIOS.find((scenario) => scenario.id === id));
  const missing = ids.filter((_, index) => !selected[index]);
  if (missing.length) throw new Error(`Unknown scenarios: ${missing.join(", ")}`);
  return selected;
}

async function readExistingRecords() {
  try {
    const text = await readFile(path.join(OUTPUT_DIR, "events.jsonl"), "utf8");
    return text
      .split("\n")
      .filter(Boolean)
      .map((line) => JSON.parse(line));
  } catch {
    return [];
  }
}

function replayRecord(records, actionable) {
  const candidates = records.filter(
    (record) => record.classification !== "pass" && (!actionable || record.classification !== "provider"),
  );
  if (candidates.length === 0) throw new Error("No matching failed record is available to replay");
  if (!actionable) return candidates.at(-1);
  const severity = { orchestration: 6, tool: 5, artifact: 4, agent: 3, ux: 2, unknown: 1 };
  return candidates
    .map((record, index) => ({ record, index }))
    .sort(
      (left, right) =>
        (severity[right.record.classification] || 0) -
          (severity[left.record.classification] || 0) || right.index - left.index,
    )[0].record;
}

function tasksFor(providers, scenarios) {
  return providers.flatMap((provider) => scenarios.map((scenario) => ({ provider, scenario })));
}

async function runPool(tasks, concurrency, worker) {
  const queue = [...tasks];
  const workers = Array.from({ length: Math.min(concurrency, queue.length) }, async () => {
    while (queue.length > 0) {
      const task = queue.shift();
      if (task) await worker(task);
    }
  });
  await Promise.all(workers);
}

function harnessFailure(runId, provider, scenario, error) {
  const now = new Date().toISOString();
  return {
    runId,
    providerId: provider.id,
    scenarioId: scenario.id,
    domain: scenario.domain,
    startedAt: now,
    finishedAt: now,
    metrics: { durationMs: 0, firstEventMs: null, firstDeltaMs: null, toolCalls: 0, turns: 0 },
    validation: {
      ok: false,
      checks: [{ name: "harness", ok: false, detail: error instanceof Error ? error.message : String(error) }],
    },
    classification: "orchestration",
    retryCount: 0,
    manifests: [],
  };
}

export async function runMain(argv = process.argv.slice(2), environment = process.env) {
  const args = parseHarnessArgs(argv);
  const previousRecords =
    args.command === "replay" || args.command === "replay-actionable" ? await readExistingRecords() : [];
  const secrets = [
    environment.POLARIS_STRESS_MINIMAX_TOKEN,
    environment.POLARIS_STRESS_KIMI_TOKEN,
  ].filter(Boolean);
  const client = new PolarisClient({ baseUrl: args.baseUrl, secrets });
  const providerState = await client.invoke("provider_list");
  let fixture = null;
  let reportQueue = Promise.resolve();
  const records = [];
  const runId = createRunId();

  try {
    const configured = await configureProviders(client, environment);
    const selectedProviders = args.providers.map((id) => {
      const provider = configured.find((candidate) => candidate.id === id);
      if (!provider) throw new Error(`Unknown provider: ${id}`);
      return provider;
    });
    fixture = await startFixtureServer({ host: "127.0.0.1", port: 0 });

    const recordOne = async ({ provider, scenario }) => {
      let record;
      try {
        record = await runScenario(client, provider, scenario, {
          runId,
          runRoot: WORK_ROOT,
          fixtureBaseUrl: fixture.baseUrl,
          maxRetries: 1,
        });
      } catch (error) {
        record = harnessFailure(runId, provider, scenario, error);
      }
      records.push(record);
      reportQueue = reportQueue.then(() => writeReport(records, OUTPUT_DIR, { secrets }));
      await reportQueue;
      const result = record.validation.ok ? "PASS" : `FAIL/${record.classification}`;
      process.stdout.write(`${provider.id} ${scenario.id}: ${result}\n`);
      return record;
    };

    const serial = async (scenarios) => {
      const stage = [];
      for (const task of tasksFor(selectedProviders, scenarios)) stage.push(await recordOne(task));
      return stage;
    };
    const concurrent = async (scenarios, level) => {
      const stage = [];
      await runPool(tasksFor(selectedProviders, scenarios), level, async (task) => {
        stage.push(await recordOne(task));
      });
      return stage;
    };

    const explicitScenarios = args.scenarioIds.length ? selectByIds(args.scenarioIds) : null;
    if (args.command === "replay" || args.command === "replay-actionable") {
      const previous = replayRecord(previousRecords, args.command === "replay-actionable");
      const provider = selectedProviders.find((candidate) => candidate.id === previous.providerId);
      const scenario = SCENARIOS.find((candidate) => candidate.id === previous.scenarioId);
      if (!provider || !scenario) throw new Error("Recorded provider or scenario is no longer available");
      await recordOne({ provider, scenario });
    } else if (args.command === "preflight") {
      await serial(explicitScenarios || selectByIds(PREFLIGHT_IDS));
    } else if (args.command === "matrix") {
      await serial(explicitScenarios || SCENARIOS);
    } else if (args.command === "recovery") {
      await serial(explicitScenarios || selectByIds(RECOVERY_IDS));
    } else if (args.command === "concurrency") {
      const levels = args.concurrency ? [args.concurrency] : [2, 4, 8];
      for (const level of levels) {
        const stage = await concurrent(explicitScenarios || selectByIds(CONCURRENCY_IDS), level);
        if (stage.some((record) => record.classification === "orchestration")) break;
      }
    } else if (args.command === "all") {
      const preflight = await serial(selectByIds(PREFLIGHT_IDS));
      if (preflight.some((record) => !record.validation.ok)) return { records, blockedAfter: "preflight" };
      await serial(explicitScenarios || SCENARIOS);
      for (const level of [2, 4, 8]) {
        const stage = await concurrent(selectByIds(CONCURRENCY_IDS), level);
        if (stage.some((record) => record.classification === "orchestration")) break;
      }
      await serial(selectByIds(RECOVERY_IDS));
    }
    await reportQueue;
    return { records, blockedAfter: null };
  } finally {
    if (fixture) await fixture.close();
    try {
      if (providerState?.currentId) {
        await client.invoke("provider_switch", { id: providerState.currentId });
      }
    } finally {
      if (typeof providerState?.linkGlobal === "boolean") {
        await client.invoke("provider_set_link_mode", { link: providerState.linkGlobal });
      }
    }
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  runMain().catch((error) => {
    process.stderr.write(`Polaris stress harness failed: ${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  });
}

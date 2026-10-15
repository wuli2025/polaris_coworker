#!/usr/bin/env node
import { format } from "node:util";
import { loadConfig } from "./config/load.ts";
import { configureNetwork, probeNetwork } from "./network.ts";
import { Logger } from "./log.ts";
import { Redactor } from "./models/redactor.ts";
import { publicError } from "./protocol/errors.ts";
import { JsonlReader, JsonlWriter } from "./protocol/jsonl.ts";

const redactor = new Redactor();
const output = new JsonlWriter(process.stdout, redactor);
const logs = new JsonlWriter(process.stderr, redactor);
// Use the same ordered writer for structured logs, without parsing user input as logs.
const writeLog = (line: string) => logs.send(JSON.parse(line));
let logger = new Logger(() => "error", redactor, writeLog);

// SDK/resource diagnostics must never corrupt the stdout protocol.
for (const method of ["log", "info", "warn", "error", "debug"] as const) {
  console[method] = (...args: unknown[]) => {
    logger.log(method === "error" ? "error" : "debug", redactor.text(format(...args)));
  };
}

async function main(): Promise<void> {
  const args = process.argv.slice(2);
  if (args[0] === "--probe-network" && (!args[1] || args[1].startsWith("--"))) throw new Error("Expected probe target");
  const probe = args[0] === "--probe-network" ? args.splice(0, 2)[1] : undefined;
  if (args.length && (args.length !== 2 || args[0] !== "--config")) {
    throw new Error("Expected --config <path>");
  }
  const config = loadConfig(args[1] ?? process.env.POLARIS_AGENTD_CONFIG);
  configureNetwork(config.network, redactor);
  if (probe !== undefined) {
    let target = probe;
    if (!/^https?:/.test(target)) {
      const { ModelRuntime } = await import("@earendil-works/pi-coding-agent");
      const { memoryCredentials } = await import("./models/credentials.ts");
      const runtime = await ModelRuntime.create({ credentials: memoryCredentials(), modelsPath: null, allowModelNetwork: false });
      target = runtime.getModels().find((model) => model.provider === target)?.baseUrl ?? "";
    }
    output.send(await probeNetwork(target, config.network));
    await output.flush();
    return;
  }
  const { Daemon } = await import("./daemon.ts");
  const { FileCredentials } = await import("./models/file-credentials.ts");
  if (process.env.POLARIS_USAGE_DIR) config.usageDir = process.env.POLARIS_USAGE_DIR;
  const path = process.env.POLARIS_PROVIDER_STORE;
  const credentials = path ? new FileCredentials(path, redactor, {
    timeoutMs: Number(process.env.POLARIS_CREDENTIAL_LOCK_TIMEOUT_MS ?? 30000),
    maxHoldMs: Number(process.env.POLARIS_CREDENTIAL_LOCK_MAX_HOLD_MS ?? 600000),
    pollMs: Number(process.env.POLARIS_CREDENTIAL_LOCK_POLL_MS ?? 25),
  }) : undefined;
  const daemon = await Daemon.create(config,
    (value) => output.send(value), writeLog, { redactor, credentials });
  logger = daemon.logger;
  let shutdown: Promise<void> | undefined;
  const reader = new JsonlReader(() => daemon.config.maxLineBytes, (value) => {
    void daemon.handle(value).then((response) => output.send(response));
  }, (error) => output.send({ type: "response", id: null, ok: false, error: publicError(error) }));
  const onData = (chunk: Buffer) => reader.push(chunk);
  const stop = () => {
    if (shutdown) return shutdown;
    process.stdin.off("data", onData);
    process.stdin.pause();
    reader.end();
    shutdown = (async () => {
      await daemon.shutdown();
      logger.log("info", "Daemon stopped");
      await output.flush();
      await logs.flush();
    })();
    return shutdown;
  };
  await new Promise<void>((resolve, reject) => {
    const finish = () => { void stop().then(resolve, reject); };
    process.stdin.on("data", onData);
    process.stdin.once("end", finish);
    process.stdin.once("error", finish);
    process.once("SIGTERM", finish);
    process.once("SIGINT", finish);
  });
}

try {
  await main();
} catch (error) {
  const safe = publicError(error);
  logger.log("error", safe.message, { code: safe.code, paths: safe.paths });
  process.exitCode = 1;
  await logs.flush().catch(() => undefined);
}

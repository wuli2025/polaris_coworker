#!/usr/bin/env node
import { format } from "node:util";
import { loadConfig } from "./config/load.ts";
import { Daemon } from "./daemon.ts";
import { Logger } from "./log.ts";
import { Redactor } from "./models/credentials.ts";
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
  if (args.length && (args.length !== 2 || args[0] !== "--config")) {
    throw new Error("Expected --config <path>");
  }
  const daemon = await Daemon.create(loadConfig(args[1] ?? process.env.POLARIS_AGENTD_CONFIG),
    (value) => output.send(value), writeLog, { redactor });
  logger = daemon.logger;
  let commands = Promise.resolve();
  let shutdown: Promise<void> | undefined;
  const reader = new JsonlReader(() => daemon.config.maxLineBytes, (value) => {
    commands = commands.then(async () => { output.send(await daemon.handle(value)); });
  }, (error) => output.send({ type: "response", id: null, ok: false, error: publicError(error) }));
  const onData = (chunk: Buffer) => reader.push(chunk);
  const stop = () => {
    if (shutdown) return shutdown;
    process.stdin.off("data", onData);
    process.stdin.pause();
    reader.end();
    shutdown = (async () => {
      await commands;
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
  logger.log("error", "Daemon failed", { code: publicError(error).code });
  process.exitCode = 1;
  await logs.flush().catch(() => undefined);
}

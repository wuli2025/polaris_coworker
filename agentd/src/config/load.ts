import { readFileSync, existsSync } from "node:fs";
import { homedir } from "node:os";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { Value } from "typebox/value";
import { ConfigSchema, ConfigPatchSchema, type Config } from "./schema.ts";
import { DaemonError } from "../protocol/errors.ts";

export function mergeConfig(base: Config, patch: unknown): Config {
  if (!Value.Check(ConfigPatchSchema, patch)) {
    throw new DaemonError("INVALID_CONFIG", "Configuration does not match schema");
  }
  const result = { ...base, ...patch };
  if (!Value.Check(ConfigSchema, result)) throw new DaemonError("INVALID_CONFIG", "Invalid configuration");
  if (new Set(result.providers.map((p) => p.id)).size !== result.providers.length) {
    throw new DaemonError("INVALID_CONFIG", "Duplicate provider id");
  }
  const dir = result.sessionDir;
  result.sessionDir = resolve(dir.startsWith("~/") ? resolve(homedir(), dir.slice(2)) : dir);
  return result;
}

export function loadConfig(path?: string): Config {
  // tsc preserves src/ under dist/; support both source and compiled entrypoints.
  const candidates = ["../../config/agent.default.json", "../../../config/agent.default.json"];
  const defaults = candidates.map((p) => fileURLToPath(new URL(p, import.meta.url))).find(existsSync);
  try {
    if (!defaults) throw new Error("Missing defaults");
    const base: unknown = JSON.parse(readFileSync(defaults, "utf8"));
    if (!Value.Check(ConfigSchema, base)) throw new Error("Invalid defaults");
    return mergeConfig(base, path ? JSON.parse(readFileSync(path, "utf8")) : {});
  } catch {
    throw new DaemonError("INVALID_CONFIG", "Cannot load configuration file");
  }
}

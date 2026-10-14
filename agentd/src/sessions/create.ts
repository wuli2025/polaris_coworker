import { mkdirSync, statSync } from "node:fs";
import { join, resolve } from "node:path";
import {
  createAgentSession, DefaultResourceLoader, SessionManager, SettingsManager,
  type ExtensionFactory,
} from "@earendil-works/pi-coding-agent";
import type { Config } from "../config/schema.ts";
import type { Models } from "../models/runtime.ts";
import { DaemonError } from "../protocol/errors.ts";

export async function createSession(
  config: Config, models: Models, factories: ExtensionFactory[], sessionId?: string, requestedCwd?: string,
) {
  const runtime = models.runtime;
  mkdirSync(config.sessionDir, { recursive: true });
  const cwd = resolve(requestedCwd ?? process.cwd());
  let manager: SessionManager;
  if (sessionId) {
    // Exact lookup, never treat an untrusted session id as a filesystem path.
    if (!/^[a-zA-Z0-9-]+$/.test(sessionId)) throw new DaemonError("INVALID_SESSION_ID", "Invalid session id");
    const path = (await SessionManager.listAll(config.sessionDir)).find((info) => info.id === sessionId)?.path;
    if (!path) throw new DaemonError("SESSION_NOT_FOUND", "Persisted session not found");
    manager = SessionManager.open(path, config.sessionDir, requestedCwd ? cwd : undefined);
  } else manager = SessionManager.create(cwd, config.sessionDir);
  try {
    if (!statSync(manager.getCwd()).isDirectory()) throw new Error();
  } catch {
    throw new DaemonError("INVALID_CWD", "Working directory does not exist");
  }
  const model = runtime.getModel(config.defaultModel.provider, config.defaultModel.id);
  if (!model) throw new DaemonError("MODEL_NOT_FOUND", "Configured default model does not exist");
  const settingsManager = SettingsManager.inMemory({
    compaction: config.compaction, retry: config.retry, cacheWarming: "off",
  });
  const agentDir = join(config.sessionDir, ".resources");
  const resourceLoader = new DefaultResourceLoader({
    cwd: manager.getCwd(), agentDir, settingsManager,
    // Only host-assembled executable extensions. Standard AGENTS.md/skills still load.
    noExtensions: true, noThemes: true,
    extensionFactories: config.extensions.enabled ? factories : [],
  });
  await resourceLoader.reload();
  const { session } = await createAgentSession({
    cwd: manager.getCwd(), agentDir, model, modelRuntime: runtime,
    thinkingLevel: config.thinkingLevel, tools: config.tools,
    settingsManager, sessionManager: manager, resourceLoader,
  });
  try {
    await session.bindExtensions({ mode: "rpc" });
    return session;
  } catch (error) {
    session.dispose();
    throw error;
  }
}

import type { AgentSession } from "@earendil-works/pi-coding-agent";
import type { Config } from "../config/schema.ts";
import { DaemonError } from "../protocol/errors.ts";
import type { SessionOptions } from "../protocol/schema.ts";

export function profileTools(config: Config, profile: string): string[] {
  if (!Object.hasOwn(config.toolProfiles, profile)) {
    throw new DaemonError("TOOL_PROFILE_NOT_FOUND", "Unknown tool profile");
  }
  return config.toolProfiles[profile];
}
export function checkTools(session: AgentSession, tools: string[]): void {
  const known = new Set(session.getAllTools().map((tool) => tool.name));
  if (tools.some((name) => !known.has(name))) {
    throw new DaemonError("TOOL_PROFILE_UNAVAILABLE", "Profile contains tools unavailable in this session; reopen required");
  }
}
export function activateTools(session: AgentSession, tools: string[]): void {
  checkTools(session, tools);
  session.setActiveToolsByName(tools);
  const active = session.getActiveToolNames();
  if (active.length !== tools.length || tools.some((name) => !active.includes(name))) {
    throw new DaemonError("TOOL_PROFILE_UNAVAILABLE", "SDK cannot activate the requested tool profile");
  }
}
export async function configureSession(session: AgentSession, config: Config, options: SessionOptions): Promise<void> {
  const tools = options.toolProfile === undefined ? undefined : profileTools(config, options.toolProfile);
  if (tools) checkTools(session, tools);
  const model = options.model && session.modelRuntime.getModel(options.model.provider, options.model.id);
  if (options.model && !model) throw new DaemonError("MODEL_NOT_FOUND", "Requested model does not exist");
  if (model) {
    try { await session.setModel(model, { persist: false }); }
    catch { throw new DaemonError("MODEL_CONFIGURE_FAILED", "SDK could not select the requested model"); }
  }
  if (options.thinkingLevel !== undefined) session.setThinkingLevel(options.thinkingLevel, { persist: false });
  if (tools) activateTools(session, tools);
}

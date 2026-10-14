import type { AgentSession } from "@earendil-works/pi-coding-agent";
import { Value } from "typebox/value";
import type { Config } from "../config/schema.ts";
import { DaemonError } from "../protocol/errors.ts";
import { SessionOptionsSchema, type SessionOptions } from "../protocol/schema.ts";

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
  // The registry also accepts direct callers; validate before any SDK mutation.
  const { model: requestedModel, thinkingLevel, toolProfile } = options;
  if (!Value.Check(SessionOptionsSchema, { model: requestedModel, thinkingLevel, toolProfile })) {
    throw new DaemonError("INVALID_REQUEST", "Session options do not match schema");
  }
  const tools = options.toolProfile === undefined ? undefined : profileTools(config, options.toolProfile);
  if (tools) checkTools(session, tools);
  const model = options.model && session.modelRuntime.getModel(options.model.provider, options.model.id);
  if (options.model && !model) throw new DaemonError("MODEL_NOT_FOUND", "Requested model does not exist");
  const previous = { model: session.model, thinkingLevel: session.thinkingLevel, tools: session.getActiveToolNames() };
  if (!previous.model) throw new DaemonError("MODEL_NOT_FOUND", "Session has no model");
  if (model) {
    if (!model.input.includes("text")) {
      throw new DaemonError("MODEL_INPUT_UNSUPPORTED", "Selected model does not accept text");
    }
    const hasImages = session.messages.some((message) => "content" in message && Array.isArray(message.content)
      && message.content.some((part) => part.type === "image"));
    if (hasImages && !model.input.includes("image")) {
      throw new DaemonError("MODEL_IMAGES_UNSUPPORTED", "Selected model does not accept session images");
    }
  }
  let modelAttempted = false;
  let thinkingAttempted = false;
  let toolsAttempted = false;
  try {
    if (model) {
      modelAttempted = true;
      // setModel also changes thinking level, including before a possible failure.
      thinkingAttempted = true;
      try { await session.setModel(model, { persist: false }); }
      catch { throw new DaemonError("MODEL_CONFIGURE_FAILED", "SDK could not select the requested model"); }
    }
    if (thinkingLevel !== undefined) {
      thinkingAttempted = true;
      session.setThinkingLevel(thinkingLevel, { persist: false });
    }
    if (tools) {
      toolsAttempted = true;
      activateTools(session, tools);
    }
  } catch (error) {
    // Restore the model before thinking: the SDK clamps thinking to the current model.
    // Attempt every restoration even if an SDK rollback operation itself fails.
    let rollbackFailed = false;
    if (modelAttempted && session.model !== previous.model) {
      try { await session.setModel(previous.model, { persist: false }); }
      catch { rollbackFailed = true; }
    }
    if (thinkingAttempted) {
      try { session.setThinkingLevel(previous.thinkingLevel, { persist: false }); }
      catch { rollbackFailed = true; }
    }
    if (toolsAttempted) {
      try { activateTools(session, previous.tools); }
      catch { rollbackFailed = true; }
    }
    if (rollbackFailed) {
      throw new DaemonError("SESSION_CONFIGURE_ROLLBACK_FAILED", "SDK could not restore session settings; reopen required");
    }
    throw error;
  }
}

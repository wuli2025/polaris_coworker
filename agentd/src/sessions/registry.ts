import { modelException } from "../events/model-error.ts";
import { recordUsage } from "../events/usage-store.ts";
import { Permissions, type Decision } from "../extensions/permissions.ts";
import type { SandboxResult } from "../protocol/schema.ts";
import { Sandbox } from "../extensions/sandbox.ts";
import { webFactory } from "../extensions/web.ts";
import { subagentFactory } from "../extensions/subagent.ts";
import { randomUUID } from "node:crypto";
import type { AgentSession, ExtensionFactory } from "@earendil-works/pi-coding-agent";
import type { Config } from "../config/schema.ts";
import { EventMapper } from "../events/mapper.ts";
import type { Models } from "../models/runtime.ts";
import type { ProtocolEvent, TurnStatus } from "../protocol/events.ts";
import { DaemonError, publicError } from "../protocol/errors.ts";
import type { SessionOptions, ImageAttachment } from "../protocol/schema.ts";
import { configureSession } from "./options.ts";
import { imagesForPrompt } from "./attachments.ts";
import { createSession } from "./create.ts";

interface Run {
  id: string;
  done: Promise<void>;
  mapper: EventMapper;
  stopped?: TurnStatus;
}
interface Entry {
  session: AgentSession;
  rebuild: () => Promise<void>;
  pendingReopen?: boolean;
  pendingModel?: SessionOptions["model"];
  permissions: Permissions;
  sandbox: Sandbox;
  options: SessionOptions;
  parent?: { id: string; callId: string };
  depth: number;
  children: Set<string>;
  run?: Run;
  lastStatus?: TurnStatus;
  configuring?: Promise<void>;
  toolProfile: string;
  closing?: Promise<void>;
  idle?: NodeJS.Timeout;
  lastUsed: number;
  queueReservations: number;
}

export class SessionRegistry {
  private readonly entries = new Map<string, Entry>();
  private readonly opening = new Set<string>();
  private pendingOpens = 0;
  private runningChildren = 0;
  private stopped = false;
  constructor(
    private readonly config: () => Config,
    private readonly models: Models,
    private readonly factories: ExtensionFactory[],
    private readonly emit: (event: ProtocolEvent) => void,
    private readonly usagePersistFailed: () => void = () => {},
  ) {}
  get size(): number { return this.entries.size; }
  async open(sessionId?: string, cwd?: string, options: SessionOptions = {}, parent?: { id: string; callId: string }) {
    if (this.stopped) throw new DaemonError("SHUTTING_DOWN", "Daemon is shutting down");
    if (sessionId && (this.entries.has(sessionId) || this.opening.has(sessionId))) {
      throw new DaemonError("SESSION_ALREADY_OPEN", "Session is already open");
    }
    if (this.size + this.pendingOpens >= this.config().maxSessions) {
      throw new DaemonError("SESSION_LIMIT", "Maximum concurrent sessions reached");
    }
    this.pendingOpens++;
    if (sessionId) this.opening.add(sessionId);
    try {
      const config = this.config();
      const state = { id: "" };
      let permissions!: Permissions;
      let sandbox!: Sandbox;
      const assemble = (actualCwd: string): ExtensionFactory[] => {
        const config = this.config();
        permissions = new Permissions(options.permissionMode ?? config.permissions.defaultMode,
          actualCwd, config.permissions.timeoutMs, config.permissions.summaryChars,
          (approval) => this.extensionEvent(state.id, { event: "permission.request", targetSessionId: state.id, ...approval }),
          (resolution) => this.extensionEvent(state.id, { event: "permission.resolved", targetSessionId: state.id, ...resolution }));
        sandbox = new Sandbox(options.sandbox ?? false, actualCwd, config.sandbox,
          (request) => this.extensionEvent(state.id, { event: "sandbox.request", targetSessionId: state.id, ...request }));
        const factories: ExtensionFactory[] = [sandbox.factory(), webFactory(config.web, this.models.credentials),
          subagentFactory((callId, task, signal) => this.subagent(state.id, callId, task, signal)),
          (pi) => { pi.on("before_agent_start", (event) => {
            const prompt = this.entries.get(state.id)?.options.expertPrompt ?? options.expertPrompt;
            if (config.experts.enabled) return { systemPrompt: [event.systemPrompt, config.experts.prompt, prompt].filter(Boolean).join("\n\n") };
          }); }, permissions.factory()];
        return factories;
      };
      const session = await createSession(config, this.models, this.factories, sessionId, cwd, options, assemble);
      state.id = session.sessionId;
      if (this.stopped) {
        session.dispose();
        throw new DaemonError("SHUTTING_DOWN", "Daemon is shutting down");
      }
      const parentEntry = parent ? this.entries.get(parent.id) : undefined;
      if (parent && (!parentEntry?.run || parentEntry.run.stopped || parentEntry.closing)) {
        session.dispose();
        throw new DaemonError("TURN_CANCELLED", "Parent stopped while opening child");
      }
      const id = session.sessionId;
      const entry: Entry = { session, permissions, sandbox, rebuild: async () => {
        options = { ...entry.options, ...(entry.pendingModel ? { model: entry.pendingModel } : {}) };
        const replacement = await createSession(this.config(), this.models, this.factories,
          undefined, entry.session.sessionManager.getCwd(), options, assemble, entry.session.sessionManager);
        const previous = entry.session;
        entry.session = replacement;
        entry.options = options;
        entry.pendingModel = undefined;
        entry.permissions = permissions;
        entry.sandbox = sandbox;
        entry.pendingReopen = replacement.modelRuntime !== this.models.runtime;
        previous.dispose();
      }, pendingReopen: session.modelRuntime !== this.models.runtime, options: { ...options }, parent,
        depth: parentEntry ? parentEntry.depth + 1 : 0, children: new Set(),
        toolProfile: options.toolProfile ?? config.defaultToolProfile, lastUsed: Date.now(), queueReservations: 0 };
      this.entries.set(id, entry);
      if (parent) this.get(parent.id).children.add(id);
      this.armIdle(id, entry);
      return { sessionId: id, resumed: !!sessionId, messageCount: session.messages.length,
        model: { provider: session.model!.provider, id: session.model!.id },
        thinkingLevel: session.thinkingLevel, toolProfile: entry.toolProfile, tools: session.getActiveToolNames(),
        reopenRequired: session.modelRuntime !== this.models.runtime };
    } finally {
      this.pendingOpens--;
      if (sessionId) this.opening.delete(sessionId);
    }
  }
  private get(id: string): Entry {
    const entry = this.entries.get(id);
    if (!entry) throw new DaemonError("SESSION_NOT_FOUND", "Session is not open");
    if (entry.closing) throw new DaemonError("SESSION_CLOSING", "Session is closing");
    return entry;
  }
  async configure(id: string, options: SessionOptions) {
    const entry = this.get(id);
    if (entry.pendingReopen && !entry.run && !entry.configuring) await this.reopen(id, entry);
    if (entry.run || entry.configuring || !entry.session.isIdle) throw new DaemonError("SESSION_BUSY", "Session is busy");
    clearTimeout(entry.idle);
    entry.configuring = configureSession(entry.session, this.config(), options);
    try {
      await entry.configuring;
      entry.toolProfile = options.toolProfile ?? entry.toolProfile;
      entry.options = { ...entry.options, ...options };
      if (options.permissionMode !== undefined) entry.permissions.configure(options.permissionMode);
      if (options.sandbox !== undefined) entry.sandbox.enabled = options.sandbox;
      return { sessionId: id, model: entry.session.model && {
        provider: entry.session.model.provider, id: entry.session.model.id },
        thinkingLevel: entry.session.thinkingLevel, toolProfile: entry.toolProfile,
        tools: entry.session.getActiveToolNames() };
    } finally {
      entry.configuring = undefined;
      entry.lastUsed = Date.now();
      if (entry.pendingReopen && !entry.closing && !this.stopped) await this.reopen(id, entry);
      if (!entry.closing && !this.stopped) this.armIdle(id, entry);
    }
  }
  prompt(id: string, text: string, attachments: ImageAttachment[] = []) {
    const entry = this.get(id);
    if (entry.run || entry.configuring) throw new DaemonError("SESSION_BUSY", "Use steer or followUp while a prompt is running");
    if (entry.pendingReopen) throw new DaemonError("SESSION_REOPEN_REQUIRED", "Session must be reopened before prompting");
    const images = imagesForPrompt(attachments, this.config());
    if (images.length && !entry.session.model?.input.includes("image")) {
      throw new DaemonError("MODEL_IMAGES_UNSUPPORTED", "Selected model does not accept images");
    }
    clearTimeout(entry.idle);
    const mapper = new EventMapper(id, randomUUID(), (event) => this.emitEvent(id, event));
    const run: Run = { id: mapper.turnId, mapper, done: Promise.resolve() };
    entry.run = run;
    // Reserve synchronously; start after the command returns to permit immediate abort.
    run.done = Promise.resolve().then(() => this.execute(id, entry, run, text, images));
    return { turnId: run.id, disposition: "started" };
  }
  private async execute(id: string, entry: Entry, run: Run, text: string, images: ImageAttachment[]): Promise<void> {
    const { session } = entry;
    run.mapper.emit({ event: "turn.started" });
    const { turnIdleTimeoutMs, turnMaxMs } = this.config();
    let idleTimer: NodeJS.Timeout;
    let maxTimer: NodeJS.Timeout | undefined;
    const expire = (code: "TURN_IDLE_TIMEOUT" | "TURN_MAX_TIMEOUT") => {
      if (run.stopped) return;
      run.stopped = "timeout";
      clearTimeout(idleTimer);
      clearTimeout(maxTimer);
      run.mapper.error(code, code === "TURN_IDLE_TIMEOUT" ? "本轮长时间未收到事件，已取消。" : "本轮超过最长运行时间，已取消。");
      this.cancelTools(entry);
      void this.stopChildren(entry);
      session.clearQueue();
      void session.abort().catch(() => undefined);
    };
    const armIdle = () => {
      clearTimeout(idleTimer);
      if (!run.stopped) idleTimer = setTimeout(() => expire("TURN_IDLE_TIMEOUT"), turnIdleTimeoutMs);
    };
    let usageOrdinal = 0;
    const unsubscribe = session.subscribe((event) => {
      armIdle(); // All SDK events count, including tool progress and retry/compaction events.
      if (event.type === "message_end" && event.message.role === "assistant") {
        try { recordUsage(this.config().usageDir, id, run.id, usageOrdinal++, event.message, session.modelRuntime); }
        catch {
          this.usagePersistFailed();
          run.mapper.error("USAGE_PERSIST_FAILED", "Usage record could not be persisted");
        }
      }
      run.mapper.accept(event, !!run.stopped);
    });
    armIdle();
    if (turnMaxMs > 0) maxTimer = setTimeout(() => expire("TURN_MAX_TIMEOUT"), turnMaxMs);
    try {
      if (!run.stopped) {
        await session.prompt(text, {
          expandPromptTemplates: false, images,
          preflightResult: () => {
            if (run.stopped) throw new DaemonError("TURN_CANCELLED", "Turn was cancelled during preparation");
          },
        });
      }
    } catch (error) {
      if (!run.stopped) {
        run.mapper.status = "error";
        const safe = modelException(error) ?? publicError(error);
        run.mapper.error(safe.code, safe.message);
      }
    } finally {
      clearTimeout(idleTimer!);
      clearTimeout(maxTimer);
      unsubscribe();
      this.cancelTools(entry);
      await this.stopChildren(entry);
      session.clearQueue();
      entry.lastStatus = run.stopped ?? run.mapper.status;
      entry.run = undefined;
      entry.lastUsed = Date.now();
      if (entry.pendingReopen && !entry.closing && !this.stopped) {
        try { await this.reopen(id, entry); }
        catch { run.mapper.error("SESSION_REOPEN_FAILED", "Provider configuration will be retried before the next turn"); }
      }
      run.mapper.emit({ event: "turn.completed", status: run.stopped ?? run.mapper.status });
      if (!entry.closing && !this.stopped) this.armIdle(id, entry);
    }
  }
  async queue(id: string, text: string, method: "steer" | "followUp") {
    const entry = this.get(id);
    const run = entry.run;
    if (!run || run.stopped) throw new DaemonError("SESSION_NOT_RUNNING", "No active turn accepts queued input");
    if (!entry.session.isStreaming) throw new DaemonError("TURN_NOT_READY", "Turn is still preparing");
    if (entry.session.pendingMessageCount + entry.queueReservations >= this.config().maxQueuedMessages) {
      throw new DaemonError("QUEUE_LIMIT", "Maximum queued messages reached");
    }
    entry.queueReservations++;
    try {
      const disposition = await entry.session[method](text);
      return { turnId: run.id, disposition };
    } finally {
      entry.queueReservations--;
      if (entry.run !== run || run.stopped) entry.session.clearQueue();
    }
  }
  async abort(id: string): Promise<{ aborted: boolean }> {
    const entry = this.get(id);
    const run = entry.run;
    if (!run) return { aborted: false };
    run.stopped ??= "aborted";
    this.cancelTools(entry);
    await this.stopChildren(entry);
    entry.session.clearQueue();
    await entry.session.abort();
    await run.done;
    return { aborted: true };
  }
  async wait(id: string): Promise<void> { await this.get(id).run?.done; }
  async close(id: string): Promise<void> {
    const entry = this.entries.get(id);
    if (!entry) throw new DaemonError("SESSION_NOT_FOUND", "Session is not open");
    if (entry.closing) return entry.closing;
    clearTimeout(entry.idle);
    if (entry.run) entry.run.stopped ??= "aborted";
    this.cancelTools(entry);
    entry.closing = (async () => {
      try {
        await entry.configuring?.catch(() => undefined);
        entry.session.clearQueue();
        const aborted = entry.session.abort();
        await Promise.all([...entry.children].map((child) => this.close(child)));
        await aborted;
        await entry.run?.done;
      } finally {
        entry.session.dispose();
        this.entries.delete(id);
        if (entry.parent) this.entries.get(entry.parent.id)?.children.delete(id);
      }
    })();
    return entry.closing;
  }
  private armIdle(id: string, entry: Entry): void {
    clearTimeout(entry.idle);
    const remaining = Math.max(1, this.config().idleTimeoutMs - (Date.now() - entry.lastUsed));
    entry.idle = setTimeout(() => { void this.close(id).catch(() => undefined); }, remaining);
    entry.idle.unref();
  }
  private cancelTools(entry: Entry): void {
    entry.permissions.cancel();
    entry.sandbox.cancel();
  }
  private async stopChildren(entry: Entry): Promise<void> {
    await Promise.all([...entry.children].map((child) => this.abort(child).catch(() => undefined)));
  }
  private extensionEvent(id: string, payload: Parameters<EventMapper["emit"]>[0]): void {
    const run = this.get(id).run;
    if (!run || (run.stopped && payload.event !== "permission.resolved")) throw new DaemonError("TURN_CANCELLED", "No active turn");
    run.mapper.emit(payload);
  }
  private emitEvent(id: string, event: ProtocolEvent): void {
    const parent = this.entries.get(id)?.parent;
    if (parent) {
      if (!["tool.started", "tool.completed", "permission.request", "permission.resolved", "sandbox.request"].includes(event.event)) return;
      const run = this.entries.get(parent.id)?.run;
      if (run) this.emitEvent(parent.id, { ...event, sessionId: parent.id,
        turnId: run.id, parentCallId: event.parentCallId ?? parent.callId });
    } else this.emit(event);
  }
  respondPermission(id: string, callId: string, decision: Decision) {
    if (!this.get(id).permissions.respond(callId, decision)) throw new DaemonError("APPROVAL_NOT_FOUND", "Approval expired or does not belong to this session");
    return { accepted: true };
  }
  respondSandbox(id: string, callId: string, result: SandboxResult) {
    if (!this.get(id).sandbox.respond(callId, result)) throw new DaemonError("SANDBOX_REQUEST_NOT_FOUND", "Sandbox request expired");
    return { accepted: true };
  }
  async subagent(id: string, callId: string, task: string, signal?: AbortSignal): Promise<string> {
    const entry = this.get(id);
    const limits = this.config().subagents;
    if (entry.depth >= limits.maxDepth) throw new DaemonError("SUBAGENT_DEPTH_LIMIT", "Maximum subagent depth reached");
    if (this.runningChildren >= limits.maxConcurrent) throw new DaemonError("SUBAGENT_CONCURRENCY_LIMIT", "Maximum concurrent subagents reached");
    if (signal?.aborted || !entry.run || entry.run.stopped) throw new DaemonError("TURN_CANCELLED", "Parent turn is not active");
    this.runningChildren++;
    let childId: string | undefined;
    const abort = () => { if (childId) void this.abort(childId).catch(() => undefined); };
    signal?.addEventListener("abort", abort, { once: true });
    try {
      const child = await this.open(undefined, entry.session.sessionManager.getCwd(), {
        ...entry.options, toolProfile: entry.toolProfile,
        model: entry.session.model && { provider: entry.session.model.provider, id: entry.session.model.id },
        thinkingLevel: entry.options.thinkingLevel, permissionMode: entry.permissions.mode, sandbox: entry.sandbox.enabled,
      }, { id, callId });
      childId = child.sessionId;
      if (signal?.aborted || entry.run?.stopped || entry.closing) throw new DaemonError("TURN_CANCELLED", "Parent cancelled while opening child");
      this.prompt(childId, task);
      await this.wait(childId);
      if (signal?.aborted || entry.run?.stopped) throw new DaemonError("TURN_CANCELLED", "Parent cancelled");
      const childEntry = this.get(childId);
      if (childEntry.lastStatus !== "completed") throw new DaemonError("SUBAGENT_FAILED", "Child did not complete successfully");
      const messages = childEntry.session.messages;
      const last = [...messages].reverse().find((message) => message.role === "assistant");
      if (!last || last.role !== "assistant" || last.stopReason === "error" || last.stopReason === "aborted") {
        throw new DaemonError("SUBAGENT_FAILED", "Child did not complete successfully");
      }
      return last.content.filter((part) => part.type === "text").map((part) => part.text).join("\n").slice(0, limits.summaryChars);
    } finally {
      signal?.removeEventListener("abort", abort);
      if (childId) await this.close(childId).catch(() => undefined);
      this.runningChildren--;
    }
  }
  reopenRequired(): string[] {
    return [...this.entries].filter(([, entry]) => entry.session.modelRuntime !== this.models.runtime)
      .map(([id]) => id);
  }
  private async reopen(id: string, entry: Entry): Promise<void> {
    clearTimeout(entry.idle);
    entry.configuring = entry.rebuild();
    try { await entry.configuring; }
    finally {
      entry.configuring = undefined;
      if (!entry.closing && !this.stopped) this.armIdle(id, entry);
    }
  }
  async refreshConfig(defaultChanged = false): Promise<void> {
    for (const [id, entry] of this.entries) {
      entry.pendingReopen = entry.session.modelRuntime !== this.models.runtime;
      if (defaultChanged) entry.pendingModel = this.config().defaultModel;
      if (!entry.run && !entry.configuring && !entry.closing) {
        try { await this.reopen(id, entry); }
        catch { /* Keep pendingReopen; config.apply reports unresolved sessions. */ }
      }
    }
  }
  async shutdown(): Promise<void> {
    this.stopped = true;
    await Promise.all([...this.entries.keys()].map((id) => this.close(id)));
  }
}

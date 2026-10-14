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
  run?: Run;
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
  private stopped = false;
  constructor(
    private readonly config: () => Config,
    private readonly models: Models,
    private readonly factories: ExtensionFactory[],
    private readonly emit: (event: ProtocolEvent) => void,
  ) {}
  get size(): number { return this.entries.size; }
  async open(sessionId?: string, cwd?: string, options: SessionOptions = {}) {
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
      const session = await createSession(config, this.models, this.factories, sessionId, cwd, options);
      if (this.stopped) {
        session.dispose();
        throw new DaemonError("SHUTTING_DOWN", "Daemon is shutting down");
      }
      const id = session.sessionId;
      const entry: Entry = { session, toolProfile: options.toolProfile ?? config.defaultToolProfile, lastUsed: Date.now(), queueReservations: 0 };
      this.entries.set(id, entry);
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
    if (entry.run || entry.configuring || !entry.session.isIdle) throw new DaemonError("SESSION_BUSY", "Session is busy");
    clearTimeout(entry.idle);
    entry.configuring = configureSession(entry.session, this.config(), options);
    try {
      await entry.configuring;
      entry.toolProfile = options.toolProfile ?? entry.toolProfile;
      return { sessionId: id, model: entry.session.model && {
        provider: entry.session.model.provider, id: entry.session.model.id },
        thinkingLevel: entry.session.thinkingLevel, toolProfile: entry.toolProfile,
        tools: entry.session.getActiveToolNames() };
    } finally {
      entry.configuring = undefined;
      entry.lastUsed = Date.now();
      if (!entry.closing && !this.stopped) this.armIdle(id, entry);
    }
  }
  prompt(id: string, text: string, attachments: ImageAttachment[] = []) {
    const entry = this.get(id);
    if (entry.run || entry.configuring) throw new DaemonError("SESSION_BUSY", "Use steer or followUp while a prompt is running");
    const images = imagesForPrompt(attachments, this.config());
    if (images.length && !entry.session.model?.input.includes("image")) {
      throw new DaemonError("MODEL_IMAGES_UNSUPPORTED", "Selected model does not accept images");
    }
    clearTimeout(entry.idle);
    const mapper = new EventMapper(id, randomUUID(), this.emit);
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
      session.clearQueue();
      void session.abort().catch(() => undefined);
    };
    const armIdle = () => {
      clearTimeout(idleTimer);
      if (!run.stopped) idleTimer = setTimeout(() => expire("TURN_IDLE_TIMEOUT"), turnIdleTimeoutMs);
    };
    const unsubscribe = session.subscribe((event) => {
      armIdle(); // All SDK events count, including tool progress and retry/compaction events.
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
        const safe = publicError(error);
        run.mapper.error(safe.code, safe.message);
      }
    } finally {
      clearTimeout(idleTimer!);
      clearTimeout(maxTimer);
      unsubscribe();
      session.clearQueue();
      entry.run = undefined;
      entry.lastUsed = Date.now();
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
    entry.closing = (async () => {
      try {
        await entry.configuring?.catch(() => undefined);
        if (entry.run) {
          entry.run.stopped ??= "aborted";
          entry.session.clearQueue();
          await entry.session.abort();
          await entry.run?.done;
        }
      } finally {
        entry.session.dispose();
        this.entries.delete(id);
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
  reopenRequired(): string[] {
    return [...this.entries].filter(([, entry]) => entry.session.modelRuntime !== this.models.runtime)
      .map(([id]) => id);
  }
  refreshConfig(): void {
    for (const [id, entry] of this.entries) if (!entry.run && !entry.configuring && !entry.closing) this.armIdle(id, entry);
  }
  async shutdown(): Promise<void> {
    this.stopped = true;
    await Promise.all([...this.entries.keys()].map((id) => this.close(id)));
  }
}

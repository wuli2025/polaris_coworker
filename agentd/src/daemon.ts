import { Value } from "typebox/value";
import type { ExtensionFactory } from "@earendil-works/pi-coding-agent";
import { mergeConfig } from "./config/load.ts";
import { ConfigPatchSchema, type Config } from "./config/schema.ts";
import { validateConfig } from "./config/validate.ts";
import { extensionFactories } from "./extensions/index.ts";
import { Logger } from "./log.ts";
import { memoryCredentials, Redactor, type CredentialStore } from "./models/credentials.ts";
import { Models } from "./models/runtime.ts";
import { DaemonError, publicError } from "./protocol/errors.ts";
import type { ProtocolEvent } from "./protocol/events.ts";
import { RequestSchema, type Request, type Response } from "./protocol/schema.ts";
import { versions } from "./protocol/version.ts";
import { validateAttachments } from "./sessions/attachments.ts";
import { SessionRegistry } from "./sessions/registry.ts";

import { Logins } from "./models/logins.ts";

export class Daemon {
  readonly logins: Logins;
  readonly sessions: SessionRegistry;
  private readonly queues = new Map<string, Promise<void>>();
  private readonly pending = new Set<Promise<void>>();
  private stopping = false;
  private shutdownTask?: Promise<void>;
  private constructor(
    public config: Config,
    readonly models: Models,
    readonly logger: Logger,
    send: (value: ProtocolEvent | Response) => void,
    factories: ExtensionFactory[],
  ) {
    this.logins = new Logins(models, () => this.config.oauthTimeoutMs);
    this.sessions = new SessionRegistry(() => this.config, models, factories,
      (event) => send(models.redactor.clean(event)));
  }
  static async create(config: Config, send: (value: ProtocolEvent | Response) => void,
    log: (line: string) => void, options: {
      credentials?: CredentialStore; redactor?: Redactor; factories?: ExtensionFactory[];
    } = {}): Promise<Daemon> {
    const redactor = options.redactor ?? new Redactor();
    const models = await Models.create(config, options.credentials ?? memoryCredentials(), redactor);
    let daemon: Daemon;
    const logger = new Logger(() => daemon?.config.logLevel ?? config.logLevel, redactor, log);
    daemon = new Daemon(config, models, logger, send, options.factories ?? extensionFactories);
    return daemon;
  }
  async handle(value: unknown): Promise<Response> {
    const raw = value && typeof value === "object" ? value as Record<string, unknown> : {};
    const id = typeof raw.id === "string" && raw.id.length || Number.isInteger(raw.id)
      ? raw.id as string | number : null;
    let sessionId: string | undefined;
    try {
      if (raw.method === "config.apply" && raw.params && typeof raw.params === "object" && "config" in raw.params) {
        validateConfig(ConfigPatchSchema, raw.params.config);
      }
      if (raw.method === "prompt" && raw.params && typeof raw.params === "object" && "attachments" in raw.params) {
        validateAttachments(raw.params.attachments, this.config);
      }
      if (!Value.Check(RequestSchema, value)) {
        throw new DaemonError(raw.method === "config.apply" ? "INVALID_CONFIG" : "INVALID_REQUEST",
          "Request does not match schema");
      }
      const request: Request = value;
      sessionId = "sessionId" in request.params ? request.params.sessionId : undefined;
      this.logger.log("debug", "Request received", { requestId: request.id, sessionId });
      if (this.stopping) throw new DaemonError("SHUTTING_DOWN", "Daemon is shutting down");
      const key = request.method.endsWith(".respond") ? undefined : sessionId ? `session:${sessionId}` : request.method === "config.apply" ? "config" : undefined;
      const result = await this.schedule(key, () => this.dispatch(request));
      this.logger.log("info", "Request completed", { requestId: request.id, sessionId });
      return { type: "response", id: request.id, ok: true, result };
    } catch (error) {
      const safe = publicError(error);
      this.logger.log("warn", "Request failed", { requestId: id ?? undefined, sessionId, code: safe.code });
      return { type: "response", id, ok: false, error: safe };
    }
  }
  private schedule(key: string | undefined, work: () => Promise<unknown>): Promise<unknown> {
    const previous = key ? this.queues.get(key) : undefined;
    const task = previous ? previous.then(work) : work();
    const settled = task.then(() => undefined, () => undefined);
    this.pending.add(settled);
    if (key) this.queues.set(key, settled);
    void settled.then(() => {
      this.pending.delete(settled);
      if (key && this.queues.get(key) === settled) this.queues.delete(key);
    });
    return task;
  }
  private async dispatch(request: Request): Promise<unknown> {
    switch (request.method) {
      case "provider.auth": return this.logins.handle(request.params.provider, request.params.operation, request.params.input);
      case "permission.respond": return this.sessions.respondPermission(request.params.sessionId, request.params.callId, request.params.decision);
      case "sandbox.respond": return this.sessions.respondSandbox(request.params.sessionId, request.params.callId, request.params);
      case "ping": return { status: "ok", sessions: this.sessions.size, ...versions };
      case "models.list": return { models: this.models.list() };
      case "models.catalog": return { models: this.models.builtinCatalog };
      case "session.open": return this.sessions.open(request.params.sessionId, request.params.cwd, request.params);
      case "session.configure": return this.sessions.configure(request.params.sessionId, request.params);
      case "prompt": return this.sessions.prompt(request.params.sessionId, request.params.text, request.params.attachments);
      case "steer":
      case "followUp": return this.sessions.queue(request.params.sessionId, request.params.text, request.method);
      case "abort": return this.sessions.abort(request.params.sessionId);
      case "close":
        await this.sessions.close(request.params.sessionId);
        return { closed: true };
      case "config.apply": {
        const config = mergeConfig(this.config, request.params.config ?? {});
        await this.models.apply(config, request.params.credentials);
        const defaultChanged = JSON.stringify(this.config.defaultModel) !== JSON.stringify(config.defaultModel);
        this.config = config;
        await this.sessions.refreshConfig(defaultChanged);
        // No config echo: credentials can also be embedded in endpoint URLs.
        return { applied: true, reopenRequired: this.sessions.reopenRequired() };
      }
    }
  }
  shutdown(): Promise<void> {
    this.stopping = true;
    return this.shutdownTask ??= (async () => {
      await this.logins.shutdown();
      await Promise.all(this.pending);
      await this.sessions.shutdown();
    })();
  }
}

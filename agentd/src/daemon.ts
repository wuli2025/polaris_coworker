import { Value } from "typebox/value";
import type { ExtensionFactory } from "@earendil-works/pi-coding-agent";
import { mergeConfig } from "./config/load.ts";
import type { Config } from "./config/schema.ts";
import { extensionFactories } from "./extensions/index.ts";
import { Logger } from "./log.ts";
import { memoryCredentials, Redactor, type CredentialStore } from "./models/credentials.ts";
import { Models } from "./models/runtime.ts";
import { DaemonError, publicError } from "./protocol/errors.ts";
import type { ProtocolEvent } from "./protocol/events.ts";
import { RequestSchema, type Request, type Response } from "./protocol/schema.ts";
import { SessionRegistry } from "./sessions/registry.ts";

export class Daemon {
  readonly sessions: SessionRegistry;
  private constructor(
    public config: Config,
    readonly models: Models,
    readonly logger: Logger,
    send: (value: ProtocolEvent | Response) => void,
    factories: ExtensionFactory[],
  ) {
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
      if (!Value.Check(RequestSchema, value)) {
        throw new DaemonError(raw.method === "config.apply" ? "INVALID_CONFIG" : "INVALID_REQUEST",
          "Request does not match schema");
      }
      const request: Request = value;
      sessionId = "sessionId" in request.params ? request.params.sessionId : undefined;
      this.logger.log("debug", "Request received", { requestId: request.id, sessionId });
      const result = await this.dispatch(request);
      this.logger.log("info", "Request completed", { requestId: request.id, sessionId });
      return { type: "response", id: request.id, ok: true, result };
    } catch (error) {
      const safe = publicError(error);
      this.logger.log("warn", "Request failed", { requestId: id ?? undefined, sessionId, code: safe.code });
      return { type: "response", id, ok: false, error: safe };
    }
  }
  private async dispatch(request: Request): Promise<unknown> {
    switch (request.method) {
      case "ping": return { status: "ok", sessions: this.sessions.size };
      case "models.list": return { models: this.models.list() };
      case "session.open": return this.sessions.open(request.params.sessionId, request.params.cwd);
      case "prompt": return this.sessions.prompt(request.params.sessionId, request.params.text);
      case "steer":
      case "followUp": return this.sessions.queue(request.params.sessionId, request.params.text, request.method);
      case "abort": return this.sessions.abort(request.params.sessionId);
      case "close":
        await this.sessions.close(request.params.sessionId);
        return { closed: true };
      case "config.apply": {
        const config = mergeConfig(this.config, request.params.config ?? {});
        await this.models.apply(config, request.params.credentials);
        this.config = config;
        this.sessions.refreshConfig();
        // No config echo: credentials can also be embedded in endpoint URLs.
        return { applied: true };
      }
    }
  }
  async shutdown(): Promise<void> { await this.sessions.shutdown(); }
}

import type { AuthEvent, AuthPrompt } from "@earendil-works/pi-ai";
import type { Models } from "./runtime.ts";
import { DaemonError } from "../protocol/errors.ts";
interface Snapshot {
  status: "pending" | "ok" | "failed" | "idle";
  url?: string; userCode?: string; interval?: number; expiresIn?: number;
  prompt?: string;
}
interface Login {
  controller: AbortController;
  state: Snapshot;
  answer?: (value: string) => void;
  done: Promise<void>;
}
/** UI-neutral SDK OAuth flows. Responses contain login interaction data only. */
export class Logins {
  private readonly active = new Map<string, Login>();
  constructor(private readonly models: Models, private readonly timeoutMs: () => number) {}
  async handle(provider: string, operation: string, input?: string): Promise<Snapshot> {
    if (!["anthropic", "openai-codex"].includes(provider)) throw new DaemonError("OAUTH_UNSUPPORTED", "Unsupported OAuth provider");
    if (operation === "cancel") {
      const old = this.active.get(provider);
      old?.controller.abort();
      await old?.done;
      this.active.delete(provider);
      return { status: "idle" };
    }
    if (operation === "poll") return { ...(this.active.get(provider)?.state ?? { status: "idle" }) };
    if (operation === "respond") {
      const login = this.active.get(provider);
      if (!login?.answer || !input) throw new DaemonError("OAUTH_INPUT_NOT_PENDING", "No login prompt awaits input");
      login.answer(input);
      login.answer = undefined;
      await login.done;
      return { ...login.state };
    }
    if (operation !== "start") throw new DaemonError("INVALID_REQUEST", "Unknown OAuth operation");
    await this.handle(provider, "cancel");
    const controller = new AbortController();
    const login: Login = { controller, state: { status: "pending" }, done: Promise.resolve() };
    this.active.set(provider, login);
    let ready!: () => void;
    const available = new Promise<void>((resolve) => { ready = resolve; });
    const timeout = setTimeout(() => controller.abort(), this.timeoutMs());
    const notify = (event: AuthEvent) => {
      if (event.type === "auth_url") { login.state.url = event.url; ready(); }
      if (event.type === "device_code") {
        Object.assign(login.state, { url: event.verificationUri, userCode: event.userCode,
          interval: event.intervalSeconds, expiresIn: event.expiresInSeconds });
        ready();
      }
    };
    const prompt = async (event: AuthPrompt): Promise<string> => {
      if (event.type === "select") {
        const requested = input === "device" ? "device_code" : "browser";
        const choice = event.options.find((option) => option.id === requested);
        if (!choice) throw new Error("OAuthSelectionUnsupported");
        return choice.id;
      }
      login.state.prompt = event.type;
      ready();
      return new Promise((resolve, reject) => {
        const signal = event.signal ?? controller.signal;
        const abort = () => { login.answer = undefined; reject(new Error("OAuthCancelled")); };
        if (signal.aborted) { abort(); return; }
        signal.addEventListener("abort", abort, { once: true });
        login.answer = (value) => { signal.removeEventListener("abort", abort); resolve(value); };
      });
    };
    login.done = this.models.runtime.login(provider, "oauth", { signal: controller.signal, notify, prompt })
      .then((credential) => {
        if (credential.type === "oauth") { this.models.redactor.add(credential.access); this.models.redactor.add(credential.refresh); }
        login.state = { status: "ok" };
      }, () => { login.state = { status: "failed" }; })
      .finally(() => { clearTimeout(timeout); login.answer = undefined; ready(); });
    await available;
    return { ...login.state };
  }
  async shutdown(): Promise<void> {
    for (const login of this.active.values()) login.controller.abort();
    await Promise.all([...this.active.values()].map((login) => login.done));
  }
}

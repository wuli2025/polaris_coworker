import { ModelRuntime } from "@earendil-works/pi-coding-agent";
import type { Config, Credentials } from "../config/schema.ts";
import { type CredentialStore, Redactor } from "./credentials.ts";
import { DaemonError } from "../protocol/errors.ts";

export class Models {
  private constructor(
    public runtime: ModelRuntime,
    readonly credentials: CredentialStore,
    readonly redactor: Redactor,
    public builtinCatalog: ReturnType<Models["list"]>,
  ) {}

  static async create(config: Config, credentials: CredentialStore, redactor: Redactor): Promise<Models> {
    for (const info of await credentials.list()) {
      const credential = await credentials.read(info.providerId);
      if (credential?.type === "api_key" && credential.key) redactor.add(credential.key);
      if (credential?.type === "oauth") { redactor.add(credential.access); redactor.add(credential.refresh); }
    }
    const { runtime, catalog } = await Models.build(config, credentials);
    return new Models(runtime, credentials, redactor, catalog);
  }

  private static async build(config: Config, credentials: CredentialStore) {
    const runtime = await ModelRuntime.create({ credentials, modelsPath: null, allowModelNetwork: false });
    const builtinProviders = new Set(runtime.getModels().map((m) => m.provider));
    const catalog = runtime.getModels().filter((m) => builtinProviders.has(m.provider)).map(({ provider, id, name, api, baseUrl, input, cost, reasoning, contextWindow, maxTokens }) => ({
      provider, id, name, api, baseUrl, input, cost, reasoning, contextWindow, maxTokens,
    }));
    for (const [index, { id, ...provider }] of config.providers.entries()) {
      for (const [modelIndex, model] of provider.models.entries()) {
        if (model.maxTokens > model.contextWindow) {
          throw new DaemonError("INVALID_PROVIDER", "Output limit exceeds context window", [`/providers/${index}/models/${modelIndex}/maxTokens`]);
        }
      }
      // Preserve native model compatibility/options while allowing explicit metadata overrides.
      if (id === "anthropic" || id === "openai-codex") {
        provider.models = provider.models.map((model) => ({ ...runtime.getModel(id, model.id), ...model }));
      }
      try { runtime.registerProvider(id, provider); }
      catch { throw new DaemonError("INVALID_PROVIDER", "Provider registration failed", [`/providers/${index}`]); }
    }
    if (runtime.getError()) throw new DaemonError("INVALID_PROVIDER", "Provider configuration is invalid");
    return { runtime, catalog };
  }

  async validate(config: Config): Promise<void> {
    await Models.build(config, this.credentials);
  }

  async apply(config: Config, keys: Credentials = {}): Promise<void> {
    for (const key of Object.values(keys)) if (key) this.redactor.add(key);
    const candidate = await Models.build(config, this.credentials);
    for (const [provider, key] of Object.entries(keys)) {
      if (key === null) await this.credentials.delete(provider);
      else await this.credentials.modify(provider, async () => ({ type: "api_key", key }));
    }
    this.runtime = candidate.runtime;
    this.builtinCatalog = candidate.catalog;
  }

  list() {
    return this.runtime.getModels().map(({ provider, id, name, api, baseUrl, input, cost, reasoning, contextWindow, maxTokens }) => ({
      provider, id, name, api, baseUrl, input, cost, reasoning, contextWindow, maxTokens,
    }));
  }
}

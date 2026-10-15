import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { WAIT_MS } from "./synchronization.ts";
// Keep readiness polling on real time when a test advances the policy clock.
const pollTimer = globalThis.setTimeout;
import { registerFauxProvider } from "@earendil-works/pi-ai/compat";
import type { ExtensionFactory } from "@earendil-works/pi-coding-agent";
import { loadConfig, mergeConfig } from "../src/config/load.ts";
import type { ConfigPatch } from "../src/config/schema.ts";
import type { CredentialStore } from "../src/models/credentials.ts";
import { Daemon } from "../src/daemon.ts";
import type { ProtocolEvent } from "../src/protocol/events.ts";
import type { Response } from "../src/protocol/schema.ts";

export type Steps = Parameters<ReturnType<typeof registerFauxProvider>["setResponses"]>[0];
export function providerConfig(model: ReturnType<ReturnType<typeof registerFauxProvider>["getModel"]>) {
  if (!model) throw new Error("Missing faux model");
  const { id, name, api, baseUrl, reasoning, input, cost, contextWindow, maxTokens } = model;
  return { id: model.provider, api, baseUrl,
    models: [{ id, name, reasoning, input, cost, contextWindow, maxTokens }] };
}
export async function harness(steps: Steps = [], patch: ConfigPatch = {}, factories: ExtensionFactory[] = [], credentials?: CredentialStore) {
  const dir = mkdtempSync(join(tmpdir(), "polaris-agentd-"));
  const faux = registerFauxProvider({ models: [{ id: "test", reasoning: true }], tokensPerSecond: Infinity });
  faux.setResponses(steps);
  const model = faux.getModel();
  const config = mergeConfig(loadConfig(), {
    sessionDir: join(dir, "sessions"), usageDir: join(dir, "usage"), defaultModel: { provider: model.provider, id: model.id },
    providers: [providerConfig(model)], tools: [],
    // SDK behavior tests opt into execution; permission tests explicitly choose their mode.
    permissions: { ...loadConfig().permissions, defaultMode: "auto-all" }, ...patch,
  });
  const events: ProtocolEvent[] = [];
  const logs: string[] = [];
  const daemon = await Daemon.create(config, (value) => {
    if (value.type === "event") events.push(value);
  }, (line) => logs.push(line), { factories, credentials });
  await daemon.models.apply(config, { [model.provider]: "test-secret-key-7654321" });
  let nextId = 0;
  const request = (method: string, params: object = {}) => daemon.handle({ id: ++nextId, method, params });
  return { dir, faux, daemon, events, logs, request,
    async close() {
      await daemon.shutdown();
      faux.unregister();
      rmSync(dir, { recursive: true, force: true });
    },
  };
}
export function result<T>(response: Response): T {
  if (!response.ok) throw new Error(JSON.stringify(response));
  return response.result as T;
}
export async function until(predicate: () => boolean): Promise<void> {
  const end = performance.now() + WAIT_MS;
  while (!predicate()) {
    if (performance.now() > end) throw new Error("Timed out waiting for condition");
    await new Promise<void>((resolve) => pollTimer(resolve, 5));
  }
}

/** Pi CLI's dispatcher policy, using the same installed undici version. */
import { EventEmitter } from "node:events";
import * as undici from "undici";
import type { Config } from "./config/schema.ts";
import type { Redactor } from "./models/credentials.ts";
import { DaemonError } from "./protocol/errors.ts";

type Network = Config["network"];
const DEFAULT_IDLE_MS = 300_000;
const FAMILY_ATTEMPT_MS = 2_000;
const DEFAULT_PROBE_MS = 5_000;
let installed: undici.EnvHttpProxyAgent | undefined;
let fingerprint: string | undefined;
const originalFetch = globalThis.fetch;
let installedFetch: typeof fetch | undefined;

export function validateNetwork(network: Network): void {
  if (network?.proxy == null) return;
  try {
    const url = new URL(network.proxy);
    if (!["http:", "https:"].includes(url.protocol) || !url.hostname || url.search || url.hash
      || url.pathname !== "/") throw new Error();
  } catch {
    throw new DaemonError("INVALID_CONFIG", "代理必须是 HTTP(S) 代理地址。", ["/network/proxy"]);
  }
}
function guarded<T extends EventEmitter>(dispatcher: T): T {
  EventEmitter.prototype.on.call(dispatcher, "error", () => {});
  return dispatcher;
}
function protectProxy(value: string | undefined | null, redactor: Redactor): void {
  if (!value) return;
  redactor.add(value);
  try {
    const url = new URL(value);
    for (const part of [url.username, url.password]) {
      redactor.add(part);
      redactor.add(decodeURIComponent(part));
    }
    redactor.add(`${decodeURIComponent(url.username)}:${decodeURIComponent(url.password)}`);
  } catch { /* Invalid URLs are reported without their contents. */ }
}
function createDispatcher(network: Network, onProxyRequest = () => {}): undici.EnvHttpProxyAgent {
  const idle = network?.idleTimeoutMs ?? DEFAULT_IDLE_MS;
  try {
    return guarded(new undici.EnvHttpProxyAgent({
      httpProxy: network?.proxy ?? undefined, httpsProxy: network?.proxy ?? undefined,
      allowH2: false, proxyTunnel: true, bodyTimeout: idle, headersTimeout: idle,
      connect: { autoSelectFamilyAttemptTimeout: FAMILY_ATTEMPT_MS },
      clientFactory: (origin, options) => {
        const client = guarded(new undici.Client(origin, options));
        const dispatch = client.dispatch.bind(client);
        client.dispatch = (options, handler) => { onProxyRequest(); return dispatch(options, handler); };
        return client;
      },
      factory: (origin, options) => (options as undici.Pool.Options).connections === 1
        ? guarded(new undici.Client(origin, options))
        : guarded(new undici.Pool(origin, { ...options,
          factory: (url, opts) => guarded(new undici.Client(url, opts)) })),
    }));
  } catch {
    throw new DaemonError("INVALID_CONFIG", "无法安装 HTTP 代理分发器。", ["/network/proxy"]);
  }
}

export function configureNetwork(network: Network, redactor: Redactor): void {
  validateNetwork(network);
  const env = process.env;
  for (const value of [network?.proxy, env.http_proxy, env.HTTP_PROXY, env.https_proxy, env.HTTPS_PROXY]) {
    protectProxy(value, redactor);
  }
  const key = JSON.stringify([network, env.http_proxy, env.HTTP_PROXY, env.https_proxy, env.HTTPS_PROXY, env.no_proxy, env.NO_PROXY]);
  if (key === fingerprint) return;
  const dispatcher = createDispatcher(network);
  undici.setGlobalDispatcher(dispatcher);
  if (globalThis.fetch === (installedFetch ?? originalFetch)) {
    undici.install();
    installedFetch = globalThis.fetch;
  }
  const previous = installed;
  installed = dispatcher;
  fingerprint = key;
  void previous?.close().catch(() => {});
}

/** HEAD at the origin only: no auth, model payload, redirects, or paid inference. */
export async function probeNetwork(target: string, network: Network): Promise<{ reachable: boolean; proxy: boolean }> {
  let dispatcher: undici.EnvHttpProxyAgent | undefined;
  let usedProxy = false;
  try {
    const url = new URL(target);
    if (!["https:", "http:"].includes(url.protocol)) throw new Error();
    // Fresh connections make proxy reporting exact even after earlier model traffic.
    dispatcher = createDispatcher(network, () => { usedProxy = true; });
    const response = await undici.fetch(url.origin, { dispatcher, method: "HEAD", redirect: "manual",
      signal: AbortSignal.timeout(network?.probeTimeoutMs ?? DEFAULT_PROBE_MS) });
    await response.body?.cancel();
    return { reachable: true, proxy: usedProxy };
  } catch { return { reachable: false, proxy: usedProxy }; }
  finally { await dispatcher?.destroy().catch(() => {}); }
}

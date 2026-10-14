import { lookup } from "node:dns/promises";
import { isIP, BlockList } from "node:net";
import { request as httpRequest } from "node:http";
import { request as httpsRequest } from "node:https";
import { Type } from "typebox";
import type { ExtensionFactory } from "@earendil-works/pi-coding-agent";
import type { Config } from "../config/schema.ts";
import type { CredentialStore } from "../models/credentials.ts";

const blocked = new BlockList();
for (const [ip, bits] of [["0.0.0.0", 8], ["10.0.0.0", 8], ["100.64.0.0", 10], ["127.0.0.0", 8],
  ["169.254.0.0", 16], ["172.16.0.0", 12], ["192.0.0.0", 24], ["192.168.0.0", 16],
  ["198.18.0.0", 15], ["224.0.0.0", 4], ["240.0.0.0", 4]] as const) blocked.addSubnet(ip, bits, "ipv4");
// Only ordinary global unicast IPv6 is allowed, excluding transition ranges.
const global6 = new BlockList();
global6.addSubnet("2000::", 3, "ipv6");
for (const [ip, bits] of [["2001::", 32], ["2001:db8::", 32], ["2002::", 16]] as const) blocked.addSubnet(ip, bits, "ipv6");
export function publicIP(ip: string): boolean {
  const family = isIP(ip);
  if (family === 4) return !blocked.check(ip, "ipv4");
  return family === 6 && global6.check(ip, "ipv6") && !blocked.check(ip, "ipv6");
}
export interface HttpReply { status: number; location?: string; contentType: string; body: string }
export interface HttpAdapter {
  get(url: URL, headers: Record<string, string>, signal?: AbortSignal): Promise<HttpReply>;
}
export class SafeHttp implements HttpAdapter {
  constructor(private readonly config: Config["web"]) {}
  async get(url: URL, headers: Record<string, string> = {}, signal?: AbortSignal): Promise<HttpReply> {
    if (!["http:", "https:"].includes(url.protocol) || url.username || url.password) throw new Error("WEB_URL_REJECTED");
    const hostname = url.hostname.replace(/^\[|\]$/g, "");
    const addresses = await lookup(hostname, { all: true });
    if (!addresses.length || addresses.some(({ address }) => !publicIP(address))) throw new Error("WEB_SSRF_REJECTED");
    const pinned = addresses[0];
    return new Promise((resolve, reject) => {
      const req = (url.protocol === "https:" ? httpsRequest : httpRequest)(url, {
        headers, signal, agent: false, family: pinned.family,
        // Pin DNS validation to the connection, preventing a second lookup/rebinding.
        lookup: (_hostname, _options, callback) => callback(null, pinned.address, pinned.family),
      }, (res) => {
        const chunks: Buffer[] = [];
        let size = 0;
        res.on("data", (chunk: Buffer) => {
          size += chunk.length;
          if (size > this.config.maxBytes) req.destroy(new Error("WEB_SIZE_LIMIT"));
          else chunks.push(chunk);
        });
        res.on("error", reject);
        res.on("end", () => resolve({ status: res.statusCode ?? 0, location: res.headers.location,
          contentType: res.headers["content-type"] ?? "", body: Buffer.concat(chunks).toString("utf8") }));
      });
      req.on("error", reject);
      req.end();
    });
  }
}
export async function boundedGet(adapter: HttpAdapter, config: Config["web"], input: string,
  headers: Record<string, string> = {}, signal?: AbortSignal): Promise<HttpReply> {
  const controller = new AbortController();
  const abort = () => controller.abort();
  signal?.addEventListener("abort", abort, { once: true });
  if (signal?.aborted) controller.abort();
  let timer: NodeJS.Timeout | undefined;
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(() => { controller.abort(); reject(new Error("WEB_TIMEOUT")); }, config.timeoutMs);
  });
  const work = async () => {
    let url = new URL(input);
    const origin = url.origin;
    for (let redirects = 0; ; redirects++) {
      if (!["http:", "https:"].includes(url.protocol) || url.username || url.password) throw new Error("WEB_URL_REJECTED");
      const reply = await adapter.get(url, url.origin === origin ? headers : {}, controller.signal);
      if (Buffer.byteLength(reply.body) > config.maxBytes) throw new Error("WEB_SIZE_LIMIT");
      if ([301, 302, 303, 307, 308].includes(reply.status) && reply.location) {
        if (redirects >= config.maxRedirects) throw new Error("WEB_REDIRECT_LIMIT");
        url = new URL(reply.location, url);
        continue;
      }
      if (reply.status < 200 || reply.status >= 300) throw new Error(`WEB_HTTP_${reply.status}`);
      return reply;
    }
  };
  try { return await Promise.race([work(), timeout]); }
  finally { clearTimeout(timer); signal?.removeEventListener("abort", abort); }
}
export function htmlText(html: string): string {
  return html.replace(/<(script|style)\b[^>]*>[\s\S]*?<\/\1\s*>/gi, "")
    .replace(/<!--[\s\S]*?-->/g, "").replace(/<[^>]+>/g, " ")
    .replace(/&(?:amp|lt|gt|quot|apos|nbsp);/g, (s) => ({
      "&amp;": "&", "&lt;": "<", "&gt;": ">", "&quot;": '"', "&apos;": "'", "&nbsp;": " ",
    })[s] ?? s).replace(/\s+/g, " ").trim();
}
export interface SearchResult { title: string; url: string; description: string }
export interface SearchAdapter { search(query: string, signal?: AbortSignal): Promise<SearchResult[]> }
export class BraveSearch implements SearchAdapter {
  constructor(private readonly config: Config["web"], private readonly credentials: CredentialStore,
    private readonly http: HttpAdapter = new SafeHttp(config)) {}
  async search(query: string, signal?: AbortSignal): Promise<SearchResult[]> {
    const key = await this.credentials.read(this.config.search.credential);
    if (key?.type !== "api_key" || !key.key) throw new Error("WEB_SEARCH_CREDENTIAL_REQUIRED");
    const url = new URL(this.config.search.endpoint);
    url.searchParams.set("q", query);
    url.searchParams.set("count", String(this.config.search.maxResults));
    const response = await boundedGet(this.http, this.config, url.href,
      { "X-Subscription-Token": key.key, Accept: "application/json" }, signal);
    const value: unknown = JSON.parse(response.body);
    const rows = (value as { web?: { results?: unknown } })?.web?.results;
    if (!Array.isArray(rows)) throw new Error("WEB_SEARCH_INVALID_RESPONSE");
    return rows.slice(0, this.config.search.maxResults).map((row) => {
      if (!row || typeof row.title !== "string" || typeof row.url !== "string"
        || typeof row.description !== "string") throw new Error("WEB_SEARCH_INVALID_RESPONSE");
      if (!["http:", "https:"].includes(new URL(row.url).protocol)) throw new Error("WEB_SEARCH_INVALID_RESPONSE");
      return { title: row.title, url: row.url, description: htmlText(row.description) };
    });
  }
}
export function webFactory(config: Config["web"], credentials: CredentialStore): ExtensionFactory {
  return (pi) => {
    const search = new BraveSearch(config, credentials);
    pi.registerTool({ name: "web_fetch", label: "Fetch web page", description: "Fetch a public HTTP/HTTPS page as text",
      parameters: Type.Object({ url: Type.String() }),
      async execute(_id, args, signal) {
        const reply = await boundedGet(new SafeHttp(config), config, args.url, {}, signal);
        return { content: [{ type: "text", text: reply.contentType.includes("html") ? htmlText(reply.body) : reply.body }], details: {} };
      } });
    pi.registerTool({ name: "web_search", label: "Search the web", description: "Search public web pages",
      parameters: Type.Object({ query: Type.String({ minLength: 1 }) }),
      async execute(_id, args, signal) {
        return { content: [{ type: "text", text: JSON.stringify(await search.search(args.query, signal)) }], details: {} };
      } });
  };
}

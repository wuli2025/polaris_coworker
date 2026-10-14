import assert from "node:assert/strict";
import { test } from "node:test";
import { createServer } from "node:http";
import { SafeHttp, publicIP, boundedGet, htmlText, BraveSearch, type HttpAdapter } from "../src/extensions/web.ts";
import { loadConfig } from "../src/config/load.ts";
import { memoryCredentials } from "../src/models/credentials.ts";
const config = loadConfig().web;

test("SSRF rejects private, mapped, link-local, DNS localhost and URL credentials", async () => {
  for (const ip of ["127.0.0.1", "10.0.0.1", "172.16.1.1", "192.168.1.1", "169.254.169.254", "::1",
    "::ffff:127.0.0.1", "fc00::1", "fe80::1", "100.64.0.1", "2002:7f00:1::", "0.0.0.0"]) assert.equal(publicIP(ip), false, ip);
  for (const ip of ["8.8.8.8", "2606:4700:4700::1111"]) assert.equal(publicIP(ip), true);
  for (const url of ["http://localhost/", "http://127.0.0.1/", "http://[::1]/", "file:///etc/passwd", "http://user:pass@example.com/"]) {
    await assert.rejects(boundedGet(new SafeHttp(config), config, url), /WEB_(SSRF|URL)_REJECTED/);
  }
  let calls = 0;
  const redirect: HttpAdapter = { get: async (url, headers, signal) => {
    if (++calls === 1) return { status: 302, location: "http://127.0.0.1/", body: "", contentType: "" };
    return new SafeHttp(config).get(url, headers, signal);
  } };
  await assert.rejects(boundedGet(redirect, config, "https://public.example/"), /WEB_SSRF_REJECTED/);
});
test("local HTTP adapter contract: search credential, bounds, timeout and HTML text", async (t) => {
  let seenKey = "";
  let seenQuery = "";
  const server = createServer((req, res) => {
    seenKey = String(req.headers["x-subscription-token"] ?? "");
    seenQuery = new URL(req.url!, "http://local").searchParams.get("q") ?? "";
    if (req.url!.startsWith("/slow")) return;
    if (req.url!.startsWith("/large")) { res.end("x".repeat(1024)); return; }
    if (req.url!.startsWith("/redirect")) { res.writeHead(302, { location: "/redirect" }); res.end(); return; }
    res.end(JSON.stringify({ web: { results: [{ title: "Result", url: "https://example.com", description: "<b>Found</b>" }] } }));
  });
  await new Promise<void>((resolve, reject) => { server.once("error", reject); server.listen(0, "127.0.0.1", resolve); });
  t.after(() => { server.closeAllConnections(); server.close(); });
  const address = server.address();
  assert.ok(address && typeof address !== "string");
  const origin = `http://127.0.0.1:${address.port}`;
  // Explicit test transport only. Production SafeHttp never permits local hosts.
  const transport: HttpAdapter = { async get(url, headers, signal) {
    const res = await fetch(url, { headers, signal, redirect: "manual" });
    return { status: res.status, location: res.headers.get("location") ?? undefined,
      contentType: res.headers.get("content-type") ?? "", body: await res.text() };
  } };
  const credentials = memoryCredentials();
  await credentials.modify(config.search.credential, async () => ({ type: "api_key", key: "search-only-secret" }));
  const search = new BraveSearch({ ...config, search: { ...config.search, endpoint: origin } }, credentials, transport);
  assert.deepEqual(await search.search("test query"), [{ title: "Result", url: "https://example.com", description: "Found" }]);
  assert.equal(seenKey, "search-only-secret");
  assert.equal(seenQuery, "test query");
  await assert.rejects(boundedGet(transport, { ...config, maxBytes: 50 }, `${origin}/large`), /WEB_SIZE_LIMIT/);
  await assert.rejects(boundedGet(transport, { ...config, timeoutMs: 20 }, `${origin}/slow`), /WEB_TIMEOUT/);
  await assert.rejects(boundedGet(transport, config, `${origin}/redirect`), /WEB_REDIRECT_LIMIT/);
  assert.equal(htmlText('<style>hidden</style><p>Hello &amp; <b>world</b></p><script>bad()</script>'), "Hello & world");
});

test("search adapter and network budgets also have a socket-free deterministic contract", async () => {
  const credentials = memoryCredentials();
  await credentials.modify(config.search.credential, async () => ({ type: "api_key", key: "adapter-secret" }));
  const adapter: HttpAdapter = { async get(url, headers) {
    assert.equal(url.searchParams.get("q"), "query");
    assert.equal(headers["X-Subscription-Token"], "adapter-secret");
    return { status: 200, contentType: "application/json", body: JSON.stringify({ web: {
      results: [{ title: "title", url: "https://example.com", description: "<b>text</b>" }],
    } }) };
  } };
  assert.deepEqual(await new BraveSearch(config, credentials, adapter).search("query"), [
    { title: "title", url: "https://example.com", description: "text" },
  ]);
  const big: HttpAdapter = { async get() { return { status: 200, contentType: "", body: "oversized" }; } };
  await assert.rejects(boundedGet(big, { ...config, maxBytes: 2 }, "https://example.com"), /WEB_SIZE_LIMIT/);
  const slow: HttpAdapter = { get: () => new Promise(() => {}) };
  await assert.rejects(boundedGet(slow, { ...config, timeoutMs: 5 }, "https://example.com"), /WEB_TIMEOUT/);
  const redirect: HttpAdapter = { async get() { return { status: 302, location: "/again", contentType: "", body: "" }; } };
  await assert.rejects(boundedGet(redirect, config, "https://example.com"), /WEB_REDIRECT_LIMIT/);
  let calls = 0;
  const crossOrigin: HttpAdapter = { async get(_url, headers) {
    if (++calls === 1) return { status: 302, location: "https://other.example/", contentType: "", body: "" };
    assert.deepEqual(headers, {});
    return { status: 200, contentType: "", body: "done" };
  } };
  await boundedGet(crossOrigin, config, "https://example.com", { authorization: "secret" });
});

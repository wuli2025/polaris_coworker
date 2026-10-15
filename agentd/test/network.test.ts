import assert from "node:assert/strict";
import { test } from "node:test";
import { createServer, type Server } from "node:http";
import { connect, type Socket } from "node:net";
import { once } from "node:events";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawn } from "node:child_process";
import { loadConfig, mergeConfig } from "../src/config/load.ts";
import { Daemon } from "../src/daemon.ts";
import { Redactor } from "../src/models/credentials.ts";
import { configureNetwork } from "../src/network.ts";
import type { ProtocolEvent } from "../src/protocol/events.ts";
import { result, until } from "./helpers.ts";
import { bounded } from "./synchronization.ts";

async function listen(server: Server): Promise<number> {
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("missing address");
  return address.port;
}
test("SDK model requests use CONNECT, NO_PROXY and explicit proxy precedence; doctor uses HEAD", async () => {
  const env = { ...process.env };
  const dir = mkdtempSync(join(tmpdir(), "agentd-proxy-"));
  const sockets = new Set<Socket>();
  let requests = 0;
  const methods: string[] = [];
  const upstream = createServer((req, res) => {
    methods.push(req.method ?? "");
    if (req.method === "HEAD") { res.writeHead(401).end(); return; }
    assert.equal(req.url, "/v1/chat/completions");
    assert.equal(req.method, "POST");
    requests++;
    req.resume();
    res.writeHead(200, { "content-type": "text/event-stream" });
    const chunk = (delta: object, finish_reason: string | null) => ({ id: "test", object: "chat.completion.chunk",
      created: 1, model: "test", choices: [{ index: 0, delta, finish_reason }] });
    res.end(`data: ${JSON.stringify(chunk({ role: "assistant", content: "proxied reply" }, null))}\n\n`
      + `data: ${JSON.stringify(chunk({}, "stop"))}\n\ndata: [DONE]\n\n`);
  });
  const proxies = [createServer(), createServer()];
  const connects = [0, 0];
  const track = (socket: Socket) => { sockets.add(socket); socket.on("close", () => sockets.delete(socket)); };
  for (const server of [upstream, ...proxies]) server.on("connection", track);
  try {
    const port = await listen(upstream);
    const ports = await Promise.all(proxies.map(listen));
    proxies.forEach((proxy, index) => proxy.on("connect", (req, downstream, head) => {
      connects[index]++;
      assert.equal(req.url?.split(":").at(-1), String(port));
      const socket = connect(port, "127.0.0.1", () => {
        downstream.write("HTTP/1.1 200 Connection Established\r\n\r\n");
        if (head.length) socket.write(head);
        downstream.pipe(socket).pipe(downstream);
      });
      track(socket);
      socket.on("error", () => downstream.destroy());
      downstream.on("error", () => socket.destroy());
    }));
    for (const key of ["HTTP_PROXY", "HTTPS_PROXY", "http_proxy", "https_proxy", "NO_PROXY", "no_proxy"]) delete process.env[key];
    process.env.HTTP_PROXY = `http://127.0.0.1:${ports[0]}`;
    process.env.HTTPS_PROXY = process.env.HTTP_PROXY;
    for (const mode of ["environment", "bypass", "explicit"] as const) {
      process.env.NO_PROXY = mode === "bypass" ? "127.0.0.1" : "";
      process.env.http_proxy = process.env.HTTP_PROXY;
      const before = [...connects];
      const events: ProtocolEvent[] = [];
      const config = mergeConfig(loadConfig(), { sessionDir: join(dir, mode), usageDir: join(dir, "usage"),
        tools: [], extensions: { enabled: false }, retry: { enabled: false },
        network: mode === "explicit" ? { proxy: `http://user:proxy-secret@127.0.0.1:${ports[1]}` } : {},
        defaultModel: { provider: "proxy-test", id: "test" },
        providers: [{ id: "proxy-test", api: "openai-completions", baseUrl: `http://127.0.0.1:${port}/v1`,
          models: [{ id: "test", name: "test", reasoning: false, input: ["text"], contextWindow: 128000,
            maxTokens: 8192, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 } }] }],
      });
      const redactor = new Redactor();
      const daemon = await Daemon.create(config, (event) => { if (event.type === "event") events.push(event); }, () => {}, { redactor });
      try {
        result(await daemon.handle({ id: 1, method: "config.apply", params: { credentials: { "proxy-test": "test-key" } } }));
        const opened = result<{ sessionId: string }>(await daemon.handle({ id: 2, method: "session.open", params: { cwd: dir } }));
        result(await daemon.handle({ id: 3, method: "prompt", params: { sessionId: opened.sessionId, text: "hello" } }));
        await until(() => events.some((event) => event.event === "turn.completed"));
        assert.ok(events.some((event) => event.event === "turn.completed" && event.status === "completed"), JSON.stringify(events));
        assert.equal(events.filter((event) => event.event === "text.delta").map((event) => event.text).join(""), "proxied reply");
        assert.equal(connects[0] > before[0], mode === "environment");
        assert.equal(connects[1] > before[1], mode === "explicit");
        if (mode === "explicit") assert.ok(!redactor.text("http://user:proxy-secret@127.0.0.1 proxy-secret").includes("proxy-secret"));
      } finally { await daemon.shutdown(); }
    }
    assert.equal(requests, 3);
    process.env.NO_PROXY = "";
    const child = spawn(process.execPath, ["--import", "tsx", "src/main.ts", "--probe-network", `http://127.0.0.1:${port}/paid`], {
      env: { ...process.env, POLARIS_AGENTD_NETWORK: JSON.stringify({ proxy: `http://127.0.0.1:${ports[1]}` }) },
      stdio: ["ignore", "pipe", "pipe"],
    });
    let output = "";
    child.stdout.on("data", (data) => { output += data; });
    const [code] = await bounded(once(child, "close"), "network probe");
    assert.equal(code, 0);
    assert.deepEqual(JSON.parse(output), { reachable: true, proxy: true });
    assert.equal(methods.at(-1), "HEAD");
    assert.equal(requests, 3, "doctor must never submit model inference");
  } finally {
    process.env = env;
    configureNetwork({}, new Redactor());
    for (const socket of sockets) socket.destroy();
    await Promise.all([upstream, ...proxies].map((server) => new Promise<void>((resolve) => server.close(() => resolve()))));
    rmSync(dir, { recursive: true, force: true });
  }
});

test("invalid network fields never expose proxy credentials", () => {
  for (const raw of ["42", "null", "[]", '{"proxy":false}']) {
    assert.throws(() => loadConfig(undefined, { POLARIS_AGENTD_NETWORK: raw }));
  }
  for (const network of [{ proxy: 1 }, { proxy: "socks5://secret@localhost:80" }, { idleTimeoutMs: -1 }, { probeTimeoutMs: 0 }]) {
    assert.throws(() => mergeConfig(loadConfig(), { network }), (error: unknown) => {
      assert.ok(error instanceof Error);
      assert.ok(!error.message.includes("secret"));
      return true;
    });
  }
});

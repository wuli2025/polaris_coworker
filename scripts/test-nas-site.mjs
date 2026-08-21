import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import path from "node:path";

const siteRoot = path.resolve(process.argv[2] || "");
assert.ok(process.argv[2], "usage: node scripts/test-nas-site.mjs SITE_ROOT");

const text = (relative) => readFile(path.join(siteRoot, relative), "utf8");
const bytes = (relative) => readFile(path.join(siteRoot, relative));
const sha256 = (value) => createHash("sha256").update(value).digest("hex");

const [nasHtml, latestText, bootstrap, legacyBootstrap, baseCompose, updateCompose, envExample, fn, headers] =
  await Promise.all([
    text("nas.html"),
    text("downloads/docker/latest.json"),
    bytes("docker/nas-bootstrap.sh"),
    bytes("docker/install-r2.sh"),
    bytes("docker/current/docker-compose.yml"),
    bytes("docker/current/docker-compose.update.yml"),
    bytes("docker/current/env.server.example"),
    text("functions/downloads/[[path]].js"),
    text("_headers"),
  ]);

const nasStart = "<!-- ══════════════ NAS（置顶 · 推荐） ══════════════ -->";
const windowsStart = "<!-- ══════════════ Windows ══════════════ -->";
const start = nasHtml.indexOf(nasStart);
const end = nasHtml.indexOf(windowsStart);
assert.ok(start >= 0 && end > start, "NAS/Windows section markers are missing or out of order");
const nasSection = nasHtml.slice(start, end);
const latest = JSON.parse(latestText);

assert.match(nasSection, /旧版 2\.7 \/ 2\.8/);
assert.match(nasSection, /已经是 2\.9\.2/);
assert.match(nasSection, /nas-bootstrap\.sh/);
assert.match(nasSection, /git pull --ff-only origin main/);
assert.doesNotMatch(nasSection, /ghcr\.io\/wuli2025\/polaris:/);
assert.doesNotMatch(nasSection, /polaris-image-2\.6\.2/);
assert.doesNotMatch(nasSection, /POLARIS_(?:AUTH|UPDATER)_TOKEN|POLARIS_REQUIRE_LOGIN/);
assert.doesNotMatch(nasHtml, /NAS 镜像 v2\.6\.2/);

assert.equal(latest.version, "2.9.2");
assert.match(latest.buildRevision, /^[0-9a-f]{40}$/);
assert.match(latest.digest, /^sha256:[0-9a-f]{64}$/);
assert.equal(latest.image, "ghcr.io/wuli2025/polaris_coworker:latest");
assert.equal(latest.bootstrap, "https://llmwiki.cloud/docker/nas-bootstrap.sh");
assert.equal(latest.compose.base, "https://llmwiki.cloud/docker/current/docker-compose.yml");
assert.equal(latest.compose.update, "https://llmwiki.cloud/docker/current/docker-compose.update.yml");
assert.equal(latest.compose.env, "https://llmwiki.cloud/docker/current/env.server.example");
assert.equal(sha256(bootstrap), latest.sha256.bootstrap);
assert.equal(sha256(baseCompose), latest.sha256.composeBase);
assert.equal(sha256(updateCompose), latest.sha256.composeUpdate);
assert.equal(sha256(envExample), latest.sha256.envExample);
assert.deepEqual(bootstrap, legacyBootstrap, "legacy install-r2 path must serve the bootstrap verbatim");

assert.match(fn, /key === "docker"/);
assert.match(fn, /\/downloads\/docker\/latest\.json/);
assert.match(
  headers,
  /\/downloads\/docker\/latest\.json\s+! Content-Disposition\s+! Cache-Control\s+Cache-Control: no-store/,
);
assert.match(headers, /\/docker\/current\/\*[\s\S]*?Cache-Control: no-store/);

const functionModule = await import(`data:text/javascript;base64,${Buffer.from(fn).toString("base64")}`);
const redirect = await functionModule.onRequestGet({
  params: { path: ["docker"] },
  env: {},
  request: new Request("https://llmwiki.cloud/downloads/docker"),
  next: () => new Response("unexpected fallback", { status: 418 }),
  waitUntil: () => {},
});
assert.equal(redirect.status, 302);
assert.equal(redirect.headers.get("location"), "https://llmwiki.cloud/downloads/docker/latest.json");

console.log("NAS site contract: ok");

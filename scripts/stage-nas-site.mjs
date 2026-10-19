import { createHash } from "node:crypto";
import { chmod, mkdir, readFile, rename, stat, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

function die(message) {
  console.error(`stage-nas-site: ${message}`);
  process.exit(1);
}

function parseArgs(argv) {
  const result = {};
  for (let index = 0; index < argv.length; index += 2) {
    const key = argv[index];
    const value = argv[index + 1];
    if (!key?.startsWith("--") || value === undefined) die(`invalid argument near ${key || "<end>"}`);
    result[key.slice(2)] = value;
  }
  return result;
}

const args = parseArgs(process.argv.slice(2));
const siteRoot = args["site-root"] ? path.resolve(args["site-root"]) : "";
const revision = args.revision || "";
const digest = args.digest || "";
if (!siteRoot) die("--site-root is required");
if (!/^[0-9a-f]{40}$/.test(revision)) die("--revision must be 40 lowercase hexadecimal characters");
if (!/^sha256:[0-9a-f]{64}$/.test(digest)) die("--digest must be sha256: plus 64 lowercase hexadecimal characters");
await stat(siteRoot).catch(() => die(`site root does not exist: ${siteRoot}`));

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const packageJson = JSON.parse(await readFile(path.join(repoRoot, "package.json"), "utf8"));
const version = packageJson.version;
if (version !== "2.9.2") die(`repository version must remain 2.9.2, got ${version}`);

async function atomicWrite(target, value, mode) {
  await mkdir(path.dirname(target), { recursive: true });
  const temporary = `${target}.tmp-${process.pid}`;
  await writeFile(temporary, value);
  if (mode !== undefined) await chmod(temporary, mode);
  await rename(temporary, target);
}

const sha256 = (value) => createHash("sha256").update(value).digest("hex");
const repoBytes = (relative) => readFile(path.join(repoRoot, relative));
const siteText = (relative) => readFile(path.join(siteRoot, relative), "utf8");
const occurrenceCount = (source, marker) => source.split(marker).length - 1;

function replaceKnownBlock(source, legacy, current, label) {
  const legacyCount = occurrenceCount(source, legacy);
  const currentCount = occurrenceCount(source, current);
  if (legacyCount === 1 && currentCount === 0) return source.replace(legacy, current);
  if (legacyCount === 0 && currentCount === 1) return source;
  die(`${label} must contain exactly one legacy or current block`);
}

const [bootstrap, baseCompose, updateCompose, envExample] = await Promise.all([
  repoBytes("docker/nas-bootstrap.sh"),
  repoBytes("docker-compose.yml"),
  repoBytes("docker-compose.update.yml"),
  repoBytes(".env.server.example"),
]);

const latest = {
  schemaVersion: 1,
  version,
  buildRevision: revision,
  image: "ghcr.io/wuli2025/polaris_coworker:latest",
  digest,
  bootstrap: "https://llmwiki.cloud/docker/nas-bootstrap.sh",
  compose: {
    base: "https://llmwiki.cloud/docker/current/docker-compose.yml",
    update: "https://llmwiki.cloud/docker/current/docker-compose.update.yml",
    env: "https://llmwiki.cloud/docker/current/env.server.example",
  },
  sha256: {
    bootstrap: sha256(bootstrap),
    composeBase: sha256(baseCompose),
    composeUpdate: sha256(updateCompose),
    envExample: sha256(envExample),
  },
};
const latestText = `${JSON.stringify(latest, null, 2)}\n`;

const nasMarker = "<!-- ══════════════ NAS（置顶 · 推荐） ══════════════ -->";
const windowsMarker = "<!-- ══════════════ Windows ══════════════ -->";
const nasPath = path.join(siteRoot, "nas.html");
const [nasHtml, templateSource] = await Promise.all([
  readFile(nasPath, "utf8"),
  readFile(path.join(repoRoot, "deploy/site/nas-section.html"), "utf8"),
]);
const nasIndex = nasHtml.indexOf(nasMarker);
const windowsIndex = nasHtml.indexOf(windowsMarker);
if (nasIndex < 0 || windowsIndex <= nasIndex) die("NAS/Windows section markers are missing or out of order");
if (nasHtml.indexOf(nasMarker, nasIndex + nasMarker.length) !== -1) die("NAS marker is not unique");
if (nasHtml.indexOf(windowsMarker, windowsIndex + windowsMarker.length) !== -1) die("Windows marker is not unique");

const renderedSection = templateSource
  .replaceAll("{{VERSION}}", version)
  .replaceAll("{{REVISION}}", revision)
  .replaceAll("{{REVISION_SHORT}}", revision.slice(0, 12))
  .replaceAll("{{DIGEST}}", digest)
  .trimEnd();
if (/\{\{[A-Z_]+\}\}/.test(renderedSection)) die("NAS section still contains template placeholders");

let renderedNas = `${nasHtml.slice(0, nasIndex)}${renderedSection}\n\n${nasHtml.slice(windowsIndex)}`;
renderedNas = renderedNas.replace(/NAS 镜像 v\d+\.\d+\.\d+/g, `NAS 镜像 v${version}`);

const legacyArchFaq = '<div class="body">你的 NAS 是 ARM 芯片（部分入门款如 DS220j / DS223j）。北极星目前只支持 x86_64（Intel/AMD）的 NAS，电脑版不受此限。</div>';
const currentArchFaq = '<div class="body">官方镜像同时支持 x86_64（Intel/AMD）和 ARM64。若仍出现 <code>exec format error</code>，请先在 NAS SSH 执行 <code>uname -m</code>，并确认使用的是官网 <code>latest</code> 镜像，而不是旧的本地 tar 镜像。</div>';
const legacyStorageFaq = '<div class="body">数据在 <code>/volume1/docker/polaris/</code>（File Station 可见）。卸载：删掉 Container Manager 里的那个容器/项目即可；要连数据一起清，再删那个文件夹。</div>';
const currentStorageFaq = '<div class="body">部署文件默认在群晖的 <code>/volume1/docker/polaris-stack/</code>，其他 NAS 默认在 <code>/opt/polaris-stack/</code>；迁移会继续使用原来的 bind 路径或命名卷。卸载时在该目录执行 Compose <code>down</code>；要保留数据就不要加 <code>-v</code>，旧恢复容器也请确认数据后再手动删除。</div>';
renderedNas = replaceKnownBlock(renderedNas, legacyArchFaq, currentArchFaq, "NAS architecture FAQ");
renderedNas = replaceKnownBlock(renderedNas, legacyStorageFaq, currentStorageFaq, "NAS storage FAQ");

const functionPath = path.join(siteRoot, "functions/downloads/[[path]].js");
let functionSource = await readFile(functionPath, "utf8");
const redirectStart = "  // POLARIS_CURRENT_DOCKER_REDIRECT_START";
const redirectEnd = "  // POLARIS_CURRENT_DOCKER_REDIRECT_END";
const redirectBlock = `${redirectStart}\n  if (key === "docker") {\n    const target = new URL("/downloads/docker/latest.json", request.url);\n    return Response.redirect(target.toString(), 302);\n  }\n${redirectEnd}`;
if (functionSource.includes(redirectStart) || functionSource.includes(redirectEnd)) {
  if (occurrenceCount(functionSource, redirectStart) !== 1 || occurrenceCount(functionSource, redirectEnd) !== 1) {
    die("download redirect staging markers must be unique");
  }
  const start = functionSource.indexOf(redirectStart);
  const end = functionSource.indexOf(redirectEnd);
  if (start < 0 || end < start) die("download redirect staging markers are malformed");
  functionSource = `${functionSource.slice(0, start)}${redirectBlock}${functionSource.slice(end + redirectEnd.length)}`;
} else {
  const keyLine = '  const key = Array.isArray(params.path) ? params.path.join("/") : String(params.path || "");';
  const keyIndex = functionSource.indexOf(keyLine);
  if (keyIndex < 0) die("download function key declaration was not found");
  const insertAt = keyIndex + keyLine.length;
  functionSource = `${functionSource.slice(0, insertAt)}\n${redirectBlock}${functionSource.slice(insertAt)}`;
}

const headersPath = path.join(siteRoot, "_headers");
let headers = await readFile(headersPath, "utf8");
const headersStart = "# POLARIS_CURRENT_DOCKER_HEADERS_START";
const headersEnd = "# POLARIS_CURRENT_DOCKER_HEADERS_END";
const headersBlock = `${headersStart}\n/downloads/docker/latest.json\n  ! Content-Disposition\n  ! Cache-Control\n  Cache-Control: no-store\n/docker/current/*\n  Cache-Control: no-store\n/docker/nas-bootstrap.sh\n  Cache-Control: no-store\n/docker/install-r2.sh\n  Cache-Control: no-store\n${headersEnd}`;
if (headers.includes(headersStart) || headers.includes(headersEnd)) {
  if (occurrenceCount(headers, headersStart) !== 1 || occurrenceCount(headers, headersEnd) !== 1) {
    die("_headers staging markers must be unique");
  }
  const start = headers.indexOf(headersStart);
  const end = headers.indexOf(headersEnd);
  if (start < 0 || end < start) die("_headers staging markers are malformed");
  headers = `${headers.slice(0, start)}${headersBlock}${headers.slice(end + headersEnd.length)}`;
} else {
  headers = `${headers.trimEnd()}\n\n${headersBlock}\n`;
}

// Validate and render every destination before the first write so a malformed
// site tree cannot be left with a partially staged release.
await Promise.all([
  atomicWrite(path.join(siteRoot, "docker/nas-bootstrap.sh"), bootstrap, 0o755),
  atomicWrite(path.join(siteRoot, "docker/install-r2.sh"), bootstrap, 0o755),
  atomicWrite(path.join(siteRoot, "docker/current/docker-compose.yml"), baseCompose),
  atomicWrite(path.join(siteRoot, "docker/current/docker-compose.update.yml"), updateCompose),
  atomicWrite(path.join(siteRoot, "docker/current/env.server.example"), envExample),
  atomicWrite(path.join(siteRoot, "downloads/docker/latest.json"), latestText),
  atomicWrite(nasPath, renderedNas),
  atomicWrite(functionPath, functionSource),
  atomicWrite(headersPath, headers),
]);

console.log(`Staged NAS ${version} (${revision.slice(0, 12)}) into ${siteRoot}`);

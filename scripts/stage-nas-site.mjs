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

const [bootstrap, baseCompose, updateCompose, envExample] = await Promise.all([
  repoBytes("docker/nas-bootstrap.sh"),
  repoBytes("docker-compose.yml"),
  repoBytes("docker-compose.update.yml"),
  repoBytes(".env.server.example"),
]);

await Promise.all([
  atomicWrite(path.join(siteRoot, "docker/nas-bootstrap.sh"), bootstrap, 0o755),
  atomicWrite(path.join(siteRoot, "docker/install-r2.sh"), bootstrap, 0o755),
  atomicWrite(path.join(siteRoot, "docker/current/docker-compose.yml"), baseCompose),
  atomicWrite(path.join(siteRoot, "docker/current/docker-compose.update.yml"), updateCompose),
  atomicWrite(path.join(siteRoot, "docker/current/env.server.example"), envExample),
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
await atomicWrite(
  path.join(siteRoot, "downloads/docker/latest.json"),
  `${JSON.stringify(latest, null, 2)}\n`,
);

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
await atomicWrite(nasPath, renderedNas);

const functionPath = path.join(siteRoot, "functions/downloads/[[path]].js");
let functionSource = await readFile(functionPath, "utf8");
const redirectStart = "  // POLARIS_CURRENT_DOCKER_REDIRECT_START";
const redirectEnd = "  // POLARIS_CURRENT_DOCKER_REDIRECT_END";
const redirectBlock = `${redirectStart}\n  if (key === "docker") {\n    const target = new URL("/downloads/docker/latest.json", request.url);\n    return Response.redirect(target.toString(), 302);\n  }\n${redirectEnd}`;
if (functionSource.includes(redirectStart) || functionSource.includes(redirectEnd)) {
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
await atomicWrite(functionPath, functionSource);

const headersPath = path.join(siteRoot, "_headers");
let headers = await readFile(headersPath, "utf8");
const headersStart = "# POLARIS_CURRENT_DOCKER_HEADERS_START";
const headersEnd = "# POLARIS_CURRENT_DOCKER_HEADERS_END";
const headersBlock = `${headersStart}\n/downloads/docker/latest.json\n  Content-Disposition: inline\n  Cache-Control: no-store\n/docker/current/*\n  Cache-Control: no-store\n/docker/nas-bootstrap.sh\n  Cache-Control: no-store\n/docker/install-r2.sh\n  Cache-Control: no-store\n${headersEnd}`;
if (headers.includes(headersStart) || headers.includes(headersEnd)) {
  const start = headers.indexOf(headersStart);
  const end = headers.indexOf(headersEnd);
  if (start < 0 || end < start) die("_headers staging markers are malformed");
  headers = `${headers.slice(0, start)}${headersBlock}${headers.slice(end + headersEnd.length)}`;
} else {
  headers = `${headers.trimEnd()}\n\n${headersBlock}\n`;
}
await atomicWrite(headersPath, headers);

console.log(`Staged NAS ${version} (${revision.slice(0, 12)}) into ${siteRoot}`);

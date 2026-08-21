import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { access, mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const siteRoot = await mkdtemp(path.join(os.tmpdir(), "polaris-site-stage-"));
const revision = "1".repeat(40);
const digest = `sha256:${"2".repeat(64)}`;

function run(relative, args, expectedStatus = 0) {
  const result = spawnSync(process.execPath, [path.join(repoRoot, relative), ...args], {
    cwd: repoRoot,
    encoding: "utf8",
  });
  assert.equal(
    result.status,
    expectedStatus,
    `${relative} exited ${result.status}\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`,
  );
}

async function hashFiles(files) {
  const hash = createHash("sha256");
  for (const relative of files) {
    hash.update(relative);
    hash.update(await readFile(path.join(siteRoot, relative)));
  }
  return hash.digest("hex");
}

try {
  await mkdir(path.join(siteRoot, "functions/downloads"), { recursive: true });
  await writeFile(
    path.join(siteRoot, "nas.html"),
    `<!doctype html>\n<!-- ══════════════ NAS（置顶 · 推荐） ══════════════ -->\n<section id="nas">legacy</section>\n<!-- ══════════════ Windows ══════════════ -->\n<section id="win">Windows</section>\n<div class="body">你的 NAS 是 ARM 芯片（部分入门款如 DS220j / DS223j）。北极星目前只支持 x86_64（Intel/AMD）的 NAS，电脑版不受此限。</div>\n<div class="body">数据在 <code>/volume1/docker/polaris/</code>（File Station 可见）。卸载：删掉 Container Manager 里的那个容器/项目即可；要连数据一起清，再删那个文件夹。</div>\n<footer>NAS 镜像 v2.6.2 · Windows v2.6.0</footer>\n`,
  );
  await writeFile(
    path.join(siteRoot, "functions/downloads/[[path]].js"),
    `async function serve(context, isHead) {\n  const { params, env, request } = context;\n  const key = Array.isArray(params.path) ? params.path.join("/") : String(params.path || "");\n  if (!env.DOWNLOADS) return context.next();\n  return new Response(isHead ? null : key);\n}\nexport async function onRequestGet(context) { return serve(context, false); }\nexport async function onRequestHead(context) { return serve(context, true); }\n`,
  );
  await writeFile(
    path.join(siteRoot, "_headers"),
    `/downloads/*\n  Content-Disposition: attachment\n  Cache-Control: public, max-age=86400\n`,
  );

  const stageArgs = [
    "--site-root",
    siteRoot,
    "--revision",
    revision,
    "--digest",
    digest,
  ];
  run("scripts/stage-nas-site.mjs", stageArgs);
  run("scripts/test-nas-site.mjs", [siteRoot]);

  const generated = [
    "nas.html",
    "downloads/docker/latest.json",
    "docker/nas-bootstrap.sh",
    "docker/install-r2.sh",
    "docker/current/docker-compose.yml",
    "docker/current/docker-compose.update.yml",
    "docker/current/env.server.example",
    "functions/downloads/[[path]].js",
    "_headers",
  ];
  const firstHash = await hashFiles(generated);
  run("scripts/stage-nas-site.mjs", stageArgs);
  const secondHash = await hashFiles(generated);
  assert.equal(secondHash, firstHash, "staging the same release twice must be byte-identical");

  const badRevision = [...stageArgs];
  badRevision[3] = "BAD";
  run("scripts/stage-nas-site.mjs", badRevision, 1);
  const badDigest = [...stageArgs];
  badDigest[5] = "BAD";
  run("scripts/stage-nas-site.mjs", badDigest, 1);

  const malformedRoot = await mkdtemp(path.join(os.tmpdir(), "polaris-site-malformed-"));
  try {
    await mkdir(path.join(malformedRoot, "functions/downloads"), { recursive: true });
    await writeFile(
      path.join(malformedRoot, "nas.html"),
      `<!-- ══════════════ NAS（置顶 · 推荐） ══════════════ -->\n<!-- ══════════════ Windows ══════════════ -->\n`,
    );
    await writeFile(path.join(malformedRoot, "functions/downloads/[[path]].js"), "broken();\n");
    await writeFile(path.join(malformedRoot, "_headers"), "/downloads/*\n  Cache-Control: public\n");
    run(
      "scripts/stage-nas-site.mjs",
      ["--site-root", malformedRoot, "--revision", revision, "--digest", digest],
      1,
    );
    await assert.rejects(
      access(path.join(malformedRoot, "docker/nas-bootstrap.sh")),
      "preflight failure must not partially write download artifacts",
    );
  } finally {
    await rm(malformedRoot, { recursive: true, force: true });
  }

  console.log("NAS site staging self-test: ok");
} finally {
  await rm(siteRoot, { recursive: true, force: true });
}

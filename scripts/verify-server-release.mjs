#!/usr/bin/env node

import { access, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { constants } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawn, spawnSync } from "node:child_process";
import { createServer } from "node:net";
import { get } from "node:http";

const releaseDir = resolve(
  process.env.POLARIS_RELEASE_DIR ?? new URL("../src-tauri/target/release", import.meta.url).pathname,
);
const suffix = process.platform === "win32" ? ".exe" : "";
const server = join(releaseDir, `polaris-server${suffix}`);
const forge = join(releaseDir, `polaris-forge${suffix}`);

async function requireExecutable(path, label) {
  try {
    await access(path, process.platform === "win32" ? constants.F_OK : constants.X_OK);
  } catch {
    throw new Error(`${label} release executable is missing: ${path}`);
  }
}

async function unusedPort() {
  const listener = createServer();
  await new Promise((resolveReady, reject) => {
    listener.once("error", reject);
    listener.listen(0, "127.0.0.1", resolveReady);
  });
  const port = listener.address().port;
  await new Promise((resolveClosed) => listener.close(resolveClosed));
  return port;
}

function healthStatus(port) {
  return new Promise((resolveStatus) => {
    const request = get(`http://127.0.0.1:${port}/api/health`, (response) => {
      response.resume();
      resolveStatus(response.statusCode === 200);
    });
    request.setTimeout(500, () => request.destroy());
    request.on("error", () => resolveStatus(false));
  });
}

async function waitForHealth(port, child) {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    if (child.exitCode !== null) return false;
    if (await healthStatus(port)) return true;
    await new Promise((resolveWait) => setTimeout(resolveWait, 100));
  }
  return false;
}

async function stopChild(child) {
  if (child.exitCode !== null) return;
  child.kill("SIGTERM");
  await Promise.race([
    new Promise((resolveExit) => child.once("exit", resolveExit)),
    new Promise((resolveWait) => setTimeout(resolveWait, 3_000)),
  ]);
  if (child.exitCode === null) child.kill("SIGKILL");
}

async function main() {
  await requireExecutable(server, "polaris-server");
  await requireExecutable(forge, "polaris-forge");

  const scratch = await mkdtemp(join(tmpdir(), "polaris-server-release-"));
  try {
    const spec = join(scratch, "polaris.slides.json");
    const out = join(scratch, "smoke.pptx");
    await writeFile(
      spec,
      JSON.stringify({
        version: 1,
        title: "Server release smoke test",
        slides: [
          { layout: "title", title: "Polaris", subtitle: "Native Forge is callable" },
          { layout: "closing", title: "Ready" },
        ],
      }),
    );
    const result = spawnSync(forge, ["spec-pptx", `--spec=${spec}`, `--out=${out}`], {
      encoding: "utf8",
      timeout: 30_000,
    });
    if (result.error) throw result.error;
    if (result.status !== 0) {
      throw new Error(`polaris-forge spec-pptx failed (${result.status}): ${result.stderr.trim()}`);
    }
    const payload = JSON.parse(result.stdout);
    if (payload.ok !== true || payload.slides !== 2) {
      throw new Error(`unexpected polaris-forge result: ${result.stdout.trim()}`);
    }
    const bytes = await readFile(out);
    if (bytes.length < 1_000 || bytes.subarray(0, 2).toString("ascii") !== "PK") {
      throw new Error("polaris-forge did not produce a valid PPTX container");
    }

    const port = await unusedPort();
    const logs = [];
    const child = spawn(server, [], {
      env: {
        ...process.env,
        HOME: scratch,
        POLARIS_PORT: String(port),
        POLARIS_WEB_DIR: scratch,
        POLARIS_PERSISTENT_AGENT: "0",
      },
      stdio: ["ignore", "pipe", "pipe"],
    });
    child.stdout.on("data", (chunk) => logs.push(String(chunk)));
    child.stderr.on("data", (chunk) => logs.push(String(chunk)));
    try {
      if (!(await waitForHealth(port, child))) {
        throw new Error(`polaris-server did not become healthy: ${logs.join("").slice(-2_000)}`);
      }
      await access(
        join(scratch, "Polaris", "skills", "browser-use", "scripts", "browser_use_runner.py"),
        constants.F_OK,
      );
      await access(
        join(scratch, "Polaris", "skills", "turbo-download", "scripts", "fast_download.py"),
        constants.F_OK,
      );
    } catch (error) {
      if (error?.code === "ENOENT") {
        throw new Error("polaris-server did not seed every built-in Agent skill runtime");
      }
      throw error;
    } finally {
      await stopChild(child);
    }
  } finally {
    await rm(scratch, { recursive: true, force: true });
  }

  console.log("PASS server release includes native Forge and every built-in Agent skill runtime");
}

main().catch((error) => {
  console.error(`FAIL ${error.message}`);
  process.exitCode = 1;
});

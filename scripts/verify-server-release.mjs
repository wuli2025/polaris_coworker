#!/usr/bin/env node

import { access, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { constants } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";

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
  } finally {
    await rm(scratch, { recursive: true, force: true });
  }

  console.log("PASS server release includes a callable native Forge CLI");
}

main().catch((error) => {
  console.error(`FAIL ${error.message}`);
  process.exitCode = 1;
});

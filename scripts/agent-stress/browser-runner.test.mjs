import { execFile } from "node:child_process";
import { mkdtemp, readFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { promisify } from "node:util";

import { expect, it } from "vitest";

const execFileAsync = promisify(execFile);
const runner = new URL(
  "../../src-tauri/src/templates/skills/browser-use/scripts/browser_use_runner.py",
  import.meta.url,
).pathname;

it("runs browser-use async work in a fresh thread when the caller already has an event loop", async () => {
  const result = await execFileAsync("python3", [runner, "--self-test-event-loop"], {
    timeout: 10_000,
  });

  expect(result.stderr).toBe("");
  expect(result.stdout.trim()).toBe("event-loop-worker: ok");
});

it("exits non-zero while preserving diagnostics when browser-use has no final result", async () => {
  const outDir = await mkdtemp(path.join(tmpdir(), "polaris-browser-runner-"));
  const probe = [
    "import importlib.util, sys",
    "spec = importlib.util.spec_from_file_location('runner', sys.argv[1])",
    "runner = importlib.util.module_from_spec(spec)",
    "spec.loader.exec_module(runner)",
    "class History:",
    "    def __str__(self): return 'diagnostic-history'",
    "runner.write_result(History(), sys.argv[2])",
  ].join("\n");

  await expect(
    execFileAsync("python3", ["-c", probe, runner, outDir], { timeout: 10_000 }),
  ).rejects.toMatchObject({ stderr: expect.stringContaining("未返回最终结果") });
  await expect(readFile(path.join(outDir, "browser_use_result.txt"), "utf8")).resolves.toBe(
    "diagnostic-history",
  );
});

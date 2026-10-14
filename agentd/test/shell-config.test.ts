import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { loadConfig, mergeConfig } from "../src/config/load.ts";
import { DaemonError } from "../src/protocol/errors.ts";
import { createSession } from "../src/sessions/create.ts";
import { harness } from "./helpers.ts";

test("shell defaults, environment fallback and explicit config precedence", () => {
  const dir = mkdtempSync(join(tmpdir(), "polaris-shell-"));
  const file = join(dir, "config.json");
  try {
    assert.equal(loadConfig(undefined, {}).shellPath, null);
    for (const shellPath of [undefined, null, ""]) {
      writeFileSync(file, JSON.stringify({ shellPath }));
      assert.equal(loadConfig(file, { POLARIS_AGENTD_SHELL_PATH: "C:/MinGit/usr/bin/bash.exe" }).shellPath,
        "C:/MinGit/usr/bin/bash.exe");
    }
    writeFileSync(file, JSON.stringify({ shellPath: "C:/Custom Git/bin/bash.exe" }));
    assert.equal(loadConfig(file, { POLARIS_AGENTD_SHELL_PATH: "bundled-bash" }).shellPath, "C:/Custom Git/bin/bash.exe");
    for (const value of [1, false, {}, []]) {
      assert.throws(() => mergeConfig(loadConfig(undefined, {}), { shellPath: value }),
        (error: unknown) => error instanceof DaemonError && error.paths?.includes("/shellPath") === true);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test("shellPath reaches actual Pi session settings including tilde expansion", async () => {
  const h = await harness();
  try {
    for (const shellPath of [null, "", "~/custom-shell", "C:/Git Bash/bin/bash.exe"]) {
      const config = mergeConfig(h.daemon.config, { shellPath });
      const session = await createSession(config, h.daemon.models, [], undefined, h.dir);
      try {
        assert.equal(session.settingsManager.getShellPath(), shellPath === "~/custom-shell"
          ? join(homedir(), "custom-shell") : shellPath || undefined);
      } finally { session.dispose(); }
    }
  } finally { await h.close(); }
});

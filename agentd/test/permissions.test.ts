import assert from "node:assert/strict";
import { test } from "node:test";
import { mkdtempSync, rmSync, symlinkSync } from "node:fs";
import { pathToFileURL } from "node:url";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Permissions, withinWorkspace, type Approval, type Decision } from "../src/extensions/permissions.ts";
import type { PermissionMode } from "../src/protocol/schema.ts";
import { fauxAssistantMessage, fauxToolCall } from "@earendil-works/pi-ai";
import { harness, result, until } from "./helpers.ts";

for (const mode of ["deny", "manual", "auto-current", "auto-all"] as PermissionMode[]) {
  for (const operation of ["read", "inside", "outside", "exec"]) {
    test(`${mode} × ${operation}`, async () => {
      const cwd = mkdtempSync(join(tmpdir(), "permission-"));
      const requests: Approval[] = [];
      const gate = new Permissions(mode, cwd, 5, 1000, (event) => requests.push(event));
      const tool = operation === "exec" ? "bash" : operation === "read" ? "read" : "write";
      const args = operation === "exec" ? { command: "pwd" } : { path: operation === "outside" ? join(cwd, "../outside") : "file" };
      try {
        const allowed = await gate.check("call", tool, args);
        const auto = mode === "auto-all" || operation === "read" || (mode === "auto-current" && operation === "inside");
        assert.equal(allowed, auto);
        const ask = !auto && mode !== "deny";
        assert.equal(requests.length, ask ? 1 : 0);
      } finally { rmSync(cwd, { recursive: true, force: true }); }
    });
  }
}
for (const decision of ["allow", "deny", "allowAlways"] as Decision[]) {
  test(`manual ${decision}, exact-call persistence and reconfigure reset`, async () => {
    let count = 0;
    const gate = new Permissions("manual", tmpdir(), 30, 100, (event) => {
      count++;
      assert.ok(gate.respond(event.callId, decision));
    });
    assert.equal(await gate.check("first", "bash", { command: "pwd" }), decision !== "deny");
    assert.equal(await gate.check("second", "bash", { command: "pwd" }), decision !== "deny");
    assert.equal(count, decision === "allowAlways" ? 1 : 2);
    await gate.check("third", "bash", { command: "other" });
    assert.equal(count, decision === "allowAlways" ? 2 : 3);
    gate.configure("deny");
    assert.equal(await gate.check("fourth", "bash", { command: "pwd" }), false);
    assert.equal(gate.respond("first", "allow"), false);
  });
}
test("auto-current does not follow symlinks outside cwd; unknown tools require approval", async () => {
  const cwd = mkdtempSync(join(tmpdir(), "permission-"));
  try {
    symlinkSync(tmpdir(), join(cwd, "link"));
    assert.equal(await withinWorkspace(cwd, "link/new-file"), false);
    assert.equal(await withinWorkspace(cwd, "~/outside"), false);
    assert.equal(await withinWorkspace(cwd, `@${join(tmpdir(), "outside")}`), false);
    assert.equal(await withinWorkspace(cwd, pathToFileURL(join(tmpdir(), "outside")).href), false);
    assert.equal(await withinWorkspace(cwd, pathToFileURL(join(cwd, "inside")).href), true);
    const gate = new Permissions("auto-current", cwd, 5, 100, () => {});
    assert.equal(await gate.check("unknown", "plugin_write", {}), false);
  } finally { rmSync(cwd, { recursive: true, force: true }); }
});
test("real Pi hook blocks a visible write until protocol approval; abort cancels pending approval", async () => {
  for (const abort of [false, true]) {
    const h = await harness([
      fauxAssistantMessage([fauxToolCall("write", { path: "result.txt", content: "approved" }, { id: "write-call" })]),
      fauxAssistantMessage("done"),
    ]);
    try {
      const { sessionId } = result<any>(await h.request("session.open", { cwd: h.dir, toolProfile: "workspace", permissionMode: "manual" }));
      await h.request("prompt", { sessionId, text: "write" });
      await until(() => h.events.some((event) => event.event === "permission.request"));
      if (abort) await h.request("abort", { sessionId });
      else assert.ok((await h.request("permission.respond", { sessionId, callId: "write-call", decision: "allow" })).ok);
      await h.daemon.sessions.wait(sessionId);
      const { existsSync } = await import("node:fs");
      assert.equal(existsSync(join(h.dir, "result.txt")), !abort);
      const resolution = h.events.findIndex((event) => event.event === "permission.resolved");
      assert.ok(resolution >= 0);
      assert.ok(resolution < h.events.findIndex((event) => event.event === "turn.completed"));
      assert.equal((h.events[resolution] as { reason: string }).reason, abort ? "cancelled" : "responded");
    } finally { await h.close(); }
  }
});

test("production defaults remain manual with validated security options", async () => {
  const { loadConfig, mergeConfig } = await import("../src/config/load.ts");
  const config = loadConfig();
  assert.equal(config.permissions.defaultMode, "manual");
  assert.equal(config.experts.enabled, false);
  for (const patch of [
    { permissions: { ...config.permissions, timeoutMs: 0 } },
    { permissions: { ...config.permissions, defaultMode: "acceptEdits" } },
    { sandbox: { ...config.sandbox, shell: "unknown" } },
    { subagents: { ...config.subagents, maxDepth: -1 } },
    { web: { ...config.web, maxBytes: 0 } },
    { experts: { ...config.experts, enabled: "yes" } },
  ]) assert.throws(() => mergeConfig(config, patch));
});

test("containment rejects parent traversal before normalization and dangling symlink prefixes", async () => {
  const cwd = mkdtempSync(join(tmpdir(), "containment-"));
  try {
    symlinkSync(tmpdir(), join(cwd, "link"));
    symlinkSync(join(tmpdir(), "missing-target-unique"), join(cwd, "dangling"));
    for (const path of ["link/../x", "link\\..\\x", "./a/../b", "link/x", "dangling/x", "dangling",
      join(tmpdir(), "outside"), "C:\\outside\\x"]) assert.equal(await withinWorkspace(cwd, path), false, path);
    for (const path of ["./a/b", join(cwd, "a/b")]) assert.equal(await withinWorkspace(cwd, path), true, path);
    assert.equal(await withinWorkspace(cwd, ".\\a\\b"), process.platform === "win32");
    const approvals: Approval[] = [];
    const gate = new Permissions("auto-current", cwd, 5, 100, (request) => approvals.push(request));
    assert.equal(await gate.check("escape", "write", { path: "link/../x" }), false);
    assert.equal(approvals.length, 1);
  } finally { rmSync(cwd, { recursive: true, force: true }); }
});
test("permission expiry and turn cancellation emit exactly one resolution", async () => {
  for (const cancel of [false, true]) {
    const resolutions: unknown[] = [];
    const gate = new Permissions("manual", tmpdir(), 10, 100, (event) => {
      assert.ok(event.expiresAt > Date.now());
    }, (event) => resolutions.push(event));
    const pending = gate.check("call", "bash", { command: "pwd" });
    if (cancel) gate.cancel();
    assert.equal(await pending, false);
    assert.deepEqual(resolutions, [{ callId: "call", reason: cancel ? "cancelled" : "expired" }]);
    assert.equal(gate.respond("call", "allow"), false);
    gate.cancel();
    assert.equal(resolutions.length, 1);
  }
});

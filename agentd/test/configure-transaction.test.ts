import assert from "node:assert/strict";
import { test } from "node:test";
import { AgentSession } from "@earendil-works/pi-coding-agent";
import { harness, result } from "./helpers.ts";
import type { SessionOptions } from "../src/protocol/schema.ts";

interface Settings {
  sessionId: string;
  model: { provider: string; id: string };
  thinkingLevel: string;
  toolProfile: string;
  tools: string[];
}

async function configuredHarness() {
  const h = await harness();
  const { id, ...provider } = h.daemon.config.providers[0];
  h.daemon.models.runtime.registerProvider(id, { ...provider, models: [
    ...provider.models, { ...provider.models[0], id: "second-model" },
    { ...provider.models[0], id: "text-model", input: ["text"] },
    { ...provider.models[0], id: "image-model", input: ["image"] },
  ] });
  const opened = result<Settings>(await h.request("session.open", {
    cwd: h.dir, thinkingLevel: "low", toolProfile: "readonly",
  }));
  const before = result<Settings>(await h.request("session.configure", { sessionId: opened.sessionId }));
  const options: SessionOptions = {
    model: { provider: id, id: "second-model" }, thinkingLevel: "high", toolProfile: "workspace",
  };
  return { ...h, before, options };
}

for (const stage of ["model", "thinking", "tools", "tools-mismatch"] as const) {
  test(`configure restores all settings after partial ${stage} mutation`, async (t) => {
    const h = await configuredHarness();
    try {
      let failed = false;
      const setModel = AgentSession.prototype.setModel;
      const setThinking = AgentSession.prototype.setThinkingLevel;
      const setTools = AgentSession.prototype.setActiveToolsByName;
      if (stage === "model") {
        t.mock.method(AgentSession.prototype, "setModel", async function (this: AgentSession, ...args: Parameters<typeof setModel>) {
          await setModel.apply(this, args);
          if (!failed) { failed = true; throw new Error("injected after model mutation"); }
        });
      } else if (stage === "thinking") {
        t.mock.method(AgentSession.prototype, "setThinkingLevel", function (this: AgentSession, ...args: Parameters<typeof setThinking>) {
          setThinking.apply(this, args);
          if (!failed && args[0] === "high") { failed = true; throw new Error("injected after thinking mutation"); }
        });
      } else {
        t.mock.method(AgentSession.prototype, "setActiveToolsByName", function (this: AgentSession, tools: string[]) {
          if (!failed && stage === "tools-mismatch") {
            failed = true;
            setTools.call(this, []);
            return;
          }
          setTools.call(this, tools);
          if (!failed) { failed = true; throw new Error("injected after tools mutation"); }
        });
      }
      const response = await h.request("session.configure", { sessionId: h.before.sessionId, ...h.options });
      assert.ok(failed);
      assert.ok(!response.ok);
      assert.equal(response.error.code, stage === "model" ? "MODEL_CONFIGURE_FAILED"
        : stage === "tools-mismatch" ? "TOOL_PROFILE_UNAVAILABLE" : "INTERNAL_ERROR");
      assert.deepEqual(result(await h.request("session.configure", { sessionId: h.before.sessionId })), h.before);
      // The same request can succeed after rollback; toolProfile commits only now.
      const after = result<Settings>(await h.request("session.configure", { sessionId: h.before.sessionId, ...h.options }));
      assert.deepEqual(after.model, h.options.model);
      assert.equal(after.thinkingLevel, "high");
      assert.equal(after.toolProfile, "workspace");
      assert.ok(after.tools.includes("write"));
    } finally { t.mock.restoreAll(); await h.close(); }
  });
}

test("configure validates every option and required model input before mutating the SDK", async (t) => {
  const h = await configuredHarness();
  try {
    const modelSpy = t.mock.method(AgentSession.prototype, "setModel");
    const thinkingSpy = t.mock.method(AgentSession.prototype, "setThinkingLevel");
    const toolsSpy = t.mock.method(AgentSession.prototype, "setActiveToolsByName");
    h.daemon.config.toolProfiles.unavailable = ["not-registered"];
    for (const [options, code] of [
      [{ ...h.options, thinkingLevel: "invalid" }, "INVALID_REQUEST"],
      [{ ...h.options, toolProfile: "missing" }, "TOOL_PROFILE_NOT_FOUND"],
      [{ ...h.options, toolProfile: "unavailable" }, "TOOL_PROFILE_UNAVAILABLE"],
      [{ ...h.options, model: { ...h.before.model, id: "missing" } }, "MODEL_NOT_FOUND"],
      [{ ...h.options, model: { ...h.before.model, id: "image-model" } }, "MODEL_INPUT_UNSUPPORTED"],
    ] as const) {
      const response = await h.request("session.configure", { sessionId: h.before.sessionId, ...options });
      assert.ok(!response.ok && response.error.code === code, JSON.stringify(response));
    }
    await assert.rejects(h.daemon.sessions.configure(h.before.sessionId, {
      ...h.options, thinkingLevel: "invalid" as SessionOptions["thinkingLevel"],
    }), { code: "INVALID_REQUEST" });
    // Persisted user/tool images also constrain a later model change.
    t.mock.getter(AgentSession.prototype, "messages", () => [{ role: "user", content: [
      { type: "image", data: "YWJj", mimeType: "image/png" },
    ], timestamp: Date.now() }]);
    const response = await h.request("session.configure", {
      sessionId: h.before.sessionId, ...h.options, model: { ...h.before.model, id: "text-model" },
    });
    assert.ok(!response.ok && response.error.code === "MODEL_IMAGES_UNSUPPORTED");
    assert.equal(modelSpy.mock.callCount(), 0);
    assert.equal(thinkingSpy.mock.callCount(), 0);
    assert.equal(toolsSpy.mock.callCount(), 0);
    assert.deepEqual(result(await h.request("session.configure", { sessionId: h.before.sessionId })), h.before);
  } finally { t.mock.restoreAll(); await h.close(); }
});

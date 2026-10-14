import assert from "node:assert/strict";
import { test } from "node:test";
import { fauxAssistantMessage } from "@earendil-works/pi-ai";
import { registerFauxProvider } from "@earendil-works/pi-ai/compat";
import { harness, providerConfig, result, until } from "./helpers.ts";

test("provider apply reports idle/running sessions to reopen; reopened sessions use new provider", async () => {
  let release!: (message: ReturnType<typeof fauxAssistantMessage>) => void;
  const h = await harness([fauxAssistantMessage("persist idle"), () => new Promise((resolve) => { release = resolve; })]);
  const next = registerFauxProvider({ models: [{ id: "new-model" }], tokensPerSecond: Infinity });
  try {
    const idle = result<{ sessionId: string }>(await h.request("session.open", { cwd: h.dir })).sessionId;
    await h.request("prompt", { sessionId: idle, text: "persist history" });
    await h.daemon.sessions.wait(idle);
    const running = result<{ sessionId: string }>(await h.request("session.open", { cwd: h.dir })).sessionId;
    await h.request("prompt", { sessionId: running, text: "old provider turn" });
    await until(() => !!release);
    const model = next.getModel();
    const applied = result<{ applied: boolean; reopenRequired: string[] }>(await h.request("config.apply", {
      config: { providers: [providerConfig(model)], defaultModel: { provider: model.provider, id: model.id } },
      credentials: { [model.provider]: "next-provider-key" },
    }));
    assert.equal(applied.applied, true);
    assert.deepEqual(applied.reopenRequired.sort(), [idle, running].sort());
    assert.equal(next.state.callCount, 0);
    release(fauxAssistantMessage("old provider finished"));
    await h.daemon.sessions.wait(running);
    assert.equal(h.events.filter((event) => event.sessionId === running).filter((event) => event.event === "text.delta").map((event) => event.text).join(""), "old provider finished");
    next.setResponses([fauxAssistantMessage("new provider finished"), fauxAssistantMessage("new provider finished")]);
    for (const id of [idle, running]) {
      await h.request("close", { sessionId: id });
      const reopened = result<{ model: { provider: string }; reopenRequired: boolean }>(
        await h.request("session.open", { sessionId: id }));
      assert.equal(reopened.model.provider, model.provider);
      assert.equal(reopened.reopenRequired, false);
      await h.request("prompt", { sessionId: id, text: "new provider turn" });
      await h.daemon.sessions.wait(id);
    }
    assert.equal(next.state.callCount, 2);
    assert.equal(h.faux.state.callCount, 2);
    assert.deepEqual(h.daemon.sessions.reopenRequired(), []);
  } finally { release?.(fauxAssistantMessage("cleanup")); await h.close(); next.unregister(); }
});

test("apply racing resource loading keeps model/runtime consistent and flags the opening session", async () => {
  let release!: () => void;
  const h = await harness([], { extensions: { enabled: true } }, [async () => {
    await new Promise<void>((resolve) => { release = resolve; });
  }]);
  try {
    const opening = h.request("session.open", { cwd: h.dir });
    await until(() => !!release);
    assert.ok((await h.request("config.apply", { config: { providers: [] } })).ok);
    release();
    const opened = result<{ sessionId: string; model: { provider: string }; reopenRequired: boolean }>(await opening);
    assert.equal(opened.model.provider, h.faux.getModel().provider);
    assert.equal(opened.reopenRequired, true);
    assert.deepEqual(h.daemon.sessions.reopenRequired(), [opened.sessionId]);
  } finally { release?.(); await h.close(); }
});

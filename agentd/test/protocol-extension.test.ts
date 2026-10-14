import assert from "node:assert/strict";
import { test } from "node:test";
import { Value } from "typebox/value";
import { readFileSync } from "node:fs";
import { fauxAssistantMessage } from "@earendil-works/pi-ai";
import { harness, result, until } from "./helpers.ts";
import { RequestSchema, PingSchema, type ImageAttachment } from "../src/protocol/schema.ts";

const image: ImageAttachment = { type: "image", mimeType: "image/png",
  data: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC" };
test("schema accepts v2 options and rejects arbitrary tools and invalid levels", () => {
  for (const method of ["session.open", "session.configure"]) {
    const params = { sessionId: "s", model: { provider: "p", id: "m" }, thinkingLevel: "high", toolProfile: "readonly" };
    assert.ok(Value.Check(RequestSchema, { id: 1, method, params }));
    for (const bad of [{ thinkingLevel: "huge" }, { toolProfile: ["read"] }, { tools: ["read"] }, { model: { id: "x" } }]) {
      assert.ok(!Value.Check(RequestSchema, { id: 1, method, params: { ...params, ...bad } }));
    }
  }
  assert.ok(Value.Check(RequestSchema, { id: 1, method: "prompt", params: { sessionId: "s", text: "look", attachments: [image] } }));
  assert.ok(!Value.Check(RequestSchema, { id: 1, method: "prompt", params: { sessionId: "s", text: "look", attachments: [{ ...image, type: "file" }] } }));
});
test("shared request samples validate against the authoritative TypeScript schema", () => {
  const path = new URL("../../src-tauri/crates/polaris-kernel/tests/fixtures/protocol-samples.jsonl", import.meta.url);
  for (const line of readFileSync(path, "utf8").trim().split("\n")) {
    const sample = JSON.parse(line);
    if (sample.method) assert.ok(Value.Check(RequestSchema, sample), line);
  }
});
test("ping versions, default options, idle configure and isolated tool/model changes", async () => {
  const h = await harness();
  try {
    const provider = h.daemon.config.providers[0];
    const { id, ...definition } = provider;
    h.daemon.models.runtime.registerProvider(id, { ...definition,
      models: [...definition.models, { ...definition.models[0], id: "second-model" }] });
    assert.ok(Value.Check(PingSchema, result(await h.request("ping"))));
    const first = result<any>(await h.request("session.open", { cwd: h.dir }));
    assert.deepEqual(first.model, h.daemon.config.defaultModel);
    assert.equal(first.thinkingLevel, h.daemon.config.thinkingLevel);
    assert.equal(first.toolProfile, h.daemon.config.defaultToolProfile);
    assert.deepEqual(first.tools, []);
    const second = result<any>(await h.request("session.open", { cwd: h.dir,
      model: first.model, toolProfile: "readonly", thinkingLevel: "low" }));
    const configured = result<any>(await h.request("session.configure", {
      sessionId: first.sessionId, model: { provider: id, id: "second-model" }, thinkingLevel: "high", toolProfile: "workspace",
    }));
    assert.deepEqual(configured.model, { provider: id, id: "second-model" });
    assert.equal(configured.thinkingLevel, "high");
    assert.ok(configured.tools.includes("write"));
    const untouched = result<any>(await h.request("session.configure", { sessionId: second.sessionId }));
    assert.deepEqual(untouched.model, first.model);
    assert.equal(untouched.thinkingLevel, "low");
    assert.ok(!untouched.tools.includes("write"));
    const readonly = result<any>(await h.request("session.configure", { sessionId: first.sessionId, toolProfile: "readonly" }));
    assert.ok(!readonly.tools.includes("write"));
    for (const [options, code] of [
      [{ toolProfile: "missing" }, "TOOL_PROFILE_NOT_FOUND"],
      [{ model: { provider: "missing", id: "missing" } }, "MODEL_NOT_FOUND"],
    ] as const) {
      const failed = await h.request("session.configure", { sessionId: first.sessionId, ...options });
      assert.ok(!failed.ok && failed.error.code === code);
    }
    h.faux.setResponses([fauxAssistantMessage("persist")]);
    await h.request("prompt", { sessionId: first.sessionId, text: "persist" });
    await h.daemon.sessions.wait(first.sessionId);
    await h.request("close", { sessionId: first.sessionId });
    const resumed = result<any>(await h.request("session.open", { sessionId: first.sessionId, thinkingLevel: "off", toolProfile: "workspace" }));
    assert.equal(resumed.thinkingLevel, "off");
    assert.ok(resumed.tools.includes("write"));
  } finally { await h.close(); }
});
test("profile and attachment configuration is validated; unavailable tools and text-only models fail by name", async () => {
  const h = await harness();
  try {
    for (const config of [
      { defaultToolProfile: "unknown" }, { toolProfiles: { x: "read" } },
      { attachments: { ...h.daemon.config.attachments, maxCount: -1 } },
      { attachments: { ...h.daemon.config.attachments, maxBytes: 0 } },
      { attachments: { ...h.daemon.config.attachments, mimeTypes: ["image/svg+xml"] } },
    ]) {
      const failed = await h.request("config.apply", { config });
      assert.ok(!failed.ok && failed.error.code === "INVALID_CONFIG");
    }
    const sessionId = result<any>(await h.request("session.open", { cwd: h.dir })).sessionId;
    assert.ok((await h.request("config.apply", { config: {
      toolProfiles: { ...h.daemon.config.toolProfiles, unavailable: ["not-registered"] },
    } })).ok);
    const failed = await h.request("session.configure", { sessionId, toolProfile: "unavailable" });
    assert.ok(!failed.ok && failed.error.code === "TOOL_PROFILE_UNAVAILABLE");
    const provider = h.daemon.config.providers[0];
    const { id, ...definition } = provider;
    h.daemon.models.runtime.registerProvider(id, { ...definition,
      models: definition.models.map((model) => ({ ...model, input: ["text"] })) });
    const textSession = result<any>(await h.request("session.open", { cwd: h.dir })).sessionId;
    const unsupported = await h.request("prompt", { sessionId: textSession, text: "look", attachments: [image] });
    assert.ok(!unsupported.ok && unsupported.error.code === "MODEL_IMAGES_UNSUPPORTED");
  } finally { await h.close(); }
});
test("configure while running returns SESSION_BUSY and image reaches faux model messages", async () => {
  let release: (() => void) | undefined;
  let messages = "";
  const h = await harness([(context) => {
    messages = JSON.stringify(context);
    return new Promise((resolve) => { release = () => resolve(fauxAssistantMessage("seen")); });
  }]);
  try {
    const sessionId = result<any>(await h.request("session.open", { cwd: h.dir })).sessionId;
    assert.ok((await h.request("prompt", { sessionId, text: "look", attachments: [image] })).ok);
    const busy = await h.request("session.configure", { sessionId, thinkingLevel: "off" });
    assert.ok(!busy.ok && busy.error.code === "SESSION_BUSY");
    await until(() => !!release);
    // Pi may resize/re-encode images; assert the actual model-bound content.
    const context = JSON.parse(messages);
    const content = context.messages.filter((m: any) => m.role === "user").flatMap((m: any) => m.content);
    const received = content.find((part: any) => part.type === "image");
    assert.ok(received && Buffer.from(received.data, "base64").length > 0, JSON.stringify(content));
    assert.ok(messages.includes('"type":"image"'));
    assert.ok(messages.includes('"mimeType":"image/png"'));
    release!();
    await h.daemon.sessions.wait(sessionId);
  } finally { release?.(); await h.close(); }
});
test("image count, per-image/total bytes, unsupported kinds and base64 fail before a turn starts", async () => {
  const h = await harness([], { attachments: { maxCount: 2, maxBytes: 4, maxTotalBytes: 5, mimeTypes: ["image/png"] } });
  try {
    const sessionId = result<any>(await h.request("session.open", { cwd: h.dir })).sessionId;
    const small = { ...image, data: "YWJj" };
    for (const [attachments, code] of [
      [[small, small, small], "ATTACHMENT_COUNT_LIMIT"],
      [[image], "ATTACHMENT_SIZE_LIMIT"],
      [[small, small], "ATTACHMENT_SIZE_LIMIT"],
      [[{ ...small, mimeType: "image/svg+xml" }], "ATTACHMENT_TYPE_UNSUPPORTED"],
      [[{ ...small, type: "file" }], "ATTACHMENT_TYPE_UNSUPPORTED"],
      [[{ ...small, data: "!!!!" }], "ATTACHMENT_INVALID_BASE64"],
    ] as const) {
      const failed = await h.request("prompt", { sessionId, text: "look", attachments });
      assert.ok(!failed.ok && failed.error.code === code, JSON.stringify(failed));
    }
    assert.equal(h.events.length, 0);
    assert.equal(h.faux.state.callCount, 0);
  } finally { await h.close(); }
});

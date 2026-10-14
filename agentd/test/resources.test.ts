import assert from "node:assert/strict";
import { test } from "node:test";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fauxAssistantMessage } from "@earendil-works/pi-ai";
import { harness, result } from "./helpers.ts";
import { contextFiles } from "../src/extensions/resources.ts";

test("legacy context fallback and AGENTS priority never rename user files", async () => {
  const h = await harness();
  try {
    writeFileSync(join(h.dir, "CLAUDE.md"), "legacy-context-marker");
    assert.ok(contextFiles(h.dir, { agentsFiles: [] }).agentsFiles.some((file) => file.content === "legacy-context-marker"));
    writeFileSync(join(h.dir, "AGENTS.md"), "agents-context-marker");
    const files = contextFiles(h.dir, { agentsFiles: [] }).agentsFiles;
    assert.ok(files.some((file) => file.content === "agents-context-marker"));
    assert.ok(!files.some((file) => file.content === "legacy-context-marker"));
  } finally { await h.close(); }
});
for (const enabled of [false, true]) test(`Pi loads configured skills and experts.enabled=${enabled}`, async () => {
  let context = "";
  const h = await harness([(value) => { context = JSON.stringify(value); return fauxAssistantMessage("done"); }], {
    experts: { enabled, prompt: "expert-config-marker" },
  });
  try {
    const skills = join(h.dir, "skills", "test-skill");
    mkdirSync(skills, { recursive: true });
    writeFileSync(join(skills, "SKILL.md"), "---\nname: test-skill\ndescription: configured-skill-marker\n---\nRead this skill.\n");
    await h.request("config.apply", { config: { skills: { paths: [join(h.dir, "skills")] } } });
    const { sessionId } = result<any>(await h.request("session.open", { cwd: h.dir,
      toolProfile: "readonly", expertPrompt: "expert-host-marker" }));
    await h.request("prompt", { sessionId, text: "hello" });
    await h.daemon.sessions.wait(sessionId);
    assert.match(context, /configured-skill-marker/);
    assert.equal(context.includes("expert-config-marker"), enabled);
    assert.equal(context.includes("expert-host-marker"), enabled);
  } finally { await h.close(); }
});

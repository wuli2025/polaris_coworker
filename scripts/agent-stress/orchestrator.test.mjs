import { mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";

import { describe, expect, it, vi } from "vitest";

import {
  CONTINUE_PROMPT,
  configureProviders,
  parseHarnessArgs,
  runBatchScenario,
  runScenario,
} from "./orchestrator.mjs";

function frame(requestId, conversationId, kind, fields = {}) {
  return {
    topic: "chat:stream",
    payload: { reqId: requestId, conversationId, kind, ...fields },
  };
}

class FakeClient {
  constructor({ manifests = [], turns = [] } = {}) {
    this.calls = [];
    this.manifests = [...manifests];
    this.turns = [...turns];
    this.projectCount = 0;
    this.conversationCount = 0;
    this.requestCount = 0;
    this.onFrame = null;
  }

  async invoke(cmd, args = {}) {
    this.calls.push({ cmd, args });
    if (cmd === "provider_list") {
      return { providers: [], currentId: "before", linkGlobal: true };
    }
    if (cmd === "conv_create_project") return { id: `p-${++this.projectCount}` };
    if (cmd === "conv_create_conversation") return { id: `c-${++this.conversationCount}` };
    if (cmd === "chat_build_manifest") return this.manifests.shift() ?? null;
    if (cmd === "artifact_list") return [];
    return null;
  }

  async connectEvents(onFrame) {
    this.onFrame = onFrame;
    return () => {
      this.onFrame = null;
    };
  }

  async sendChat(args) {
    this.calls.push({ cmd: "chat_send", args: { args } });
    const requestId = `r-${++this.requestCount}`;
    const turn = this.turns.shift() || {
      text: "CY-303 costs 1599",
      tools: [{ name: "Bash", detail: "python cloakbrowser_probe.py" }],
      errors: [],
    };
    queueMicrotask(() => {
      for (const tool of turn.tools || []) {
        this.onFrame?.(frame(requestId, args.conversationId, "tool", { tool: tool.name, text: tool.detail }));
      }
      if (turn.text) {
        this.onFrame?.(frame(requestId, args.conversationId, "delta", { text: turn.text }));
      }
      for (const error of turn.errors || []) {
        this.onFrame?.(frame(requestId, args.conversationId, "error", { text: error }));
      }
      this.onFrame?.(frame(requestId, args.conversationId, "done"));
    });
    return requestId;
  }
}

function credentials() {
  return {
    POLARIS_STRESS_MINIMAX_TOKEN: "minimax-secret-for-test",
    POLARIS_STRESS_KIMI_TOKEN: "kimi-secret-for-test",
  };
}

function browserScenario() {
  return {
    id: "browser-test",
    name: "browser test",
    domain: "browser",
    timeoutMs: 2_000,
    batchBuild: false,
    batchSize: null,
    skillIds: ["cloak-browser"],
    promptTemplate: "Visit {{fixtureBaseUrl}} in {{workDir}} and return CY-303 1599",
    expectedTokens: ["CY-303", "1599"],
  };
}

describe("provider configuration", () => {
  it("configures both Anthropic-compatible providers only through Polaris", async () => {
    const client = new FakeClient();
    const directFetch = vi.spyOn(globalThis, "fetch");
    const providers = await configureProviders(client, credentials());

    const saves = client.calls.filter((call) => call.cmd === "provider_save");
    const kimi = saves.find((call) => call.args.input.id === "stress-kimi-k3");
    const minimax = saves.find((call) => call.args.input.id === "stress-minimax-m3");
    expect(providers.map((provider) => provider.id)).toEqual([
      "stress-minimax-m3",
      "stress-kimi-k3",
    ]);
    expect(kimi.args.input.protocol).toBe("");
    expect(kimi.args.input.settingsConfig.env).toMatchObject({
      ANTHROPIC_BASE_URL: "https://api.kimi.com/coding/",
      ANTHROPIC_AUTH_TOKEN: "kimi-secret-for-test",
      ANTHROPIC_MODEL: "k3",
      ANTHROPIC_DEFAULT_HAIKU_MODEL: "k3",
      ANTHROPIC_DEFAULT_SONNET_MODEL: "k3",
      ANTHROPIC_DEFAULT_OPUS_MODEL: "k3",
      CLAUDE_CODE_SUBAGENT_MODEL: "k3",
    });
    expect(minimax.args.input.settingsConfig.env.ANTHROPIC_BASE_URL).toBe(
      "https://api.minimaxi.com/anthropic",
    );
    expect(client.calls).toContainEqual({ cmd: "provider_set_link_mode", args: { link: false } });
    expect(directFetch).not.toHaveBeenCalled();
    directFetch.mockRestore();
  });

  it("reports missing environment names without including any token value", async () => {
    await expect(configureProviders(new FakeClient(), {})).rejects.toThrow(
      "POLARIS_STRESS_MINIMAX_TOKEN, POLARIS_STRESS_KIMI_TOKEN",
    );
  });

  it("configures only the selected provider and requires only its credential", async () => {
    const client = new FakeClient();
    const providers = await configureProviders(
      client,
      { POLARIS_STRESS_MINIMAX_TOKEN: "minimax-only-secret" },
      ["stress-minimax-m3"],
    );

    expect(providers.map((provider) => provider.id)).toEqual(["stress-minimax-m3"]);
    const saves = client.calls.filter((call) => call.cmd === "provider_save");
    expect(saves).toHaveLength(1);
    expect(saves[0].args.input.id).toBe("stress-minimax-m3");
    expect(saves[0].args.input.settingsConfig.env.ANTHROPIC_AUTH_TOKEN).toBe(
      "minimax-only-secret",
    );
  });
});

describe("scenario orchestration", () => {
  it("creates an isolated project and conversation and submits through chat_send", async () => {
    const client = new FakeClient();
    const runRoot = await mkdtemp(path.join(tmpdir(), "polaris-orchestrator-"));
    const record = await runScenario(
      client,
      { id: "stress-kimi-k3", name: "Kimi", model: "k3" },
      browserScenario(),
      {
        runId: "run-fixed",
        runRoot,
        fixtureBaseUrl: "http://127.0.0.1:39091",
      },
    );

    const project = client.calls.find((call) => call.cmd === "conv_create_project");
    const binding = client.calls.find((call) => call.cmd === "conv_set_project_work_dir");
    const chat = client.calls.find((call) => call.cmd === "chat_send");
    expect(project.args.name).toContain("run-fixed");
    expect(binding.args.projectId).toBe("p-1");
    expect(binding.args.workDir).toContain(path.join("run-fixed", "stress-kimi-k3", "browser-test"));
    expect(chat.args.args).toMatchObject({
      conversationId: "c-1",
      permissionMode: "auto_all",
      workMode: "work",
      providerId: "stress-kimi-k3",
      skillIds: ["cloak-browser"],
    });
    expect(record.validation.ok).toBe(true);
    expect(record.metrics.toolCalls).toBe(1);
  });

  it("backs off and retries a provider 429 without direct provider traffic", async () => {
    const client = new FakeClient({
      turns: [
        { text: "", tools: [], errors: ["upstream HTTP 429 rate limit"] },
        {
          text: "CY-303 costs 1599",
          tools: [{ name: "Bash", detail: "uv run cloakbrowser probe.py" }],
          errors: [],
        },
      ],
    });
    const runRoot = await mkdtemp(path.join(tmpdir(), "polaris-retry-"));
    const delays = [];
    const directFetch = vi.spyOn(globalThis, "fetch");
    const record = await runScenario(
      client,
      { id: "stress-minimax-m3", name: "MiniMax", model: "MiniMax-M3" },
      browserScenario(),
      {
        runId: "run-retry",
        runRoot,
        fixtureBaseUrl: "http://localhost:39091",
        maxRetries: 1,
        sleep: async (milliseconds) => delays.push(milliseconds),
      },
    );

    expect(record.retryCount).toBe(1);
    expect(record.validation.ok).toBe(true);
    expect(delays).toEqual([1_000]);
    expect(directFetch).not.toHaveBeenCalled();
    directFetch.mockRestore();
  });

  it("retries a provider overload returned as successful terminal text", async () => {
    const client = new FakeClient({
      turns: [
        {
          text: "API Error: Request rejected (429) · The engine is currently overloaded, please try again later",
          tools: [],
          errors: [],
        },
        {
          text: "CY-303 costs 1599",
          tools: [{ name: "Bash", detail: "uv run cloakbrowser probe.py" }],
          errors: [],
        },
      ],
    });
    const runRoot = await mkdtemp(path.join(tmpdir(), "polaris-text-retry-"));
    const delays = [];
    const record = await runScenario(
      client,
      { id: "stress-kimi-k3", name: "Kimi", model: "k3" },
      browserScenario(),
      {
        runId: "run-text-retry",
        runRoot,
        fixtureBaseUrl: "http://localhost:39091",
        maxRetries: 1,
        sleep: async (milliseconds) => delays.push(milliseconds),
      },
    );

    expect(record.retryCount).toBe(1);
    expect(record.previousAttempts).toEqual([
      { classification: "provider", validationOk: false },
    ]);
    expect(record.validation.ok).toBe(true);
    expect(delays).toEqual([1_000]);
  });

  it("continues a batch task with the application prompt until manifest pending reaches zero", async () => {
    const client = new FakeClient({
      manifests: [
        { units: [{ status: "done" }, { status: "pending" }, { status: "pending" }] },
        { units: [{ status: "done" }, { status: "done" }, { status: "pending" }] },
        { units: [{ status: "done" }, { status: "done" }, { status: "done" }] },
      ],
    });
    const runRoot = await mkdtemp(path.join(tmpdir(), "polaris-batch-"));
    const scenario = {
      id: "ppt-test",
      name: "ppt test",
      domain: "ppt",
      timeoutMs: 2_000,
      batchBuild: true,
      batchSize: 1,
      skillIds: ["polaris-deck-studio"],
      promptTemplate: "Build deck in {{workDir}}",
      expectedSlides: 3,
      requiredText: [],
    };

    const record = await runBatchScenario(
      client,
      { id: "stress-kimi-k3", name: "Kimi", model: "k3" },
      scenario,
      { runId: "run-batch", runRoot, fixtureBaseUrl: "http://localhost:39091" },
    );

    const prompts = client.calls
      .filter((call) => call.cmd === "chat_send")
      .map((call) => call.args.args.prompt);
    expect(prompts).toEqual([
      expect.stringContaining("Build deck"),
      CONTINUE_PROMPT,
      CONTINUE_PROMPT,
      expect.stringContaining("最终交付审计"),
    ]);
    expect(record.manifests.map((manifest) => manifest.pending)).toEqual([2, 1, 0]);
  });
});

describe("CLI safety", () => {
  it("rejects credentials in flags and accepts only known commands", () => {
    expect(() => parseHarnessArgs(["matrix", "--token", "secret"])).toThrow("environment");
    expect(() => parseHarnessArgs(["unknown"])).toThrow("Unknown command");
    expect(parseHarnessArgs(["matrix", "--base-url", "http://127.0.0.1:8899"])).toMatchObject({
      command: "matrix",
      baseUrl: "http://127.0.0.1:8899",
      workMode: "work",
    });
    expect(parseHarnessArgs(["matrix", "--work-mode", "fast"])).toMatchObject({
      workMode: "fast",
    });
    expect(() => parseHarnessArgs(["matrix", "--work-mode", "turbo"])).toThrow(
      "--work-mode",
    );
  });
});

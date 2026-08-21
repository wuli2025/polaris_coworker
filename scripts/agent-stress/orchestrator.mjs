import { cp, mkdir, readdir, readFile, stat } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { RunState } from "./run-state.mjs";
import { scenarioPrompt } from "./scenarios.mjs";
import { validateBrowser, validateCode, validateGeneric, validatePptx } from "./validators.mjs";

const CODE_FIXTURE = fileURLToPath(new URL("./fixtures/code/order-service/", import.meta.url));
const KNOWN_COMMANDS = new Set([
  "preflight",
  "matrix",
  "concurrency",
  "recovery",
  "replay",
  "replay-actionable",
  "all",
]);
const CREDENTIAL_FLAG = /(?:token|api[-_]?key|secret|authorization|auth[-_]?token)/i;
const PROVIDER_RETRYABLE = /(?:\b429\b|rate.?limit|too many requests|quota|temporar|\b5\d\d\b|upstream.*(?:busy|unavailable))/i;

export const CONTINUE_PROMPT =
  "继续构建下一批。先 Read 工作目录里的 `polaris.build.json`，按清单取下一批 pending 单元构建、" +
  "回写状态、增量落盘；本批做完即停，末尾报进度。若全部 done 则做收尾并写 `BUILD COMPLETE`。";

export const PROVIDERS = [
  {
    id: "stress-minimax-m3",
    name: "MiniMax M3 · Polaris Stress",
    baseUrl: "https://api.minimaxi.com/anthropic",
    websiteUrl: "https://www.minimaxi.com",
    model: "MiniMax-M3",
    tokenEnv: "POLARIS_STRESS_MINIMAX_TOKEN",
  },
  {
    id: "stress-kimi-k3",
    name: "Kimi K3 · Polaris Stress",
    baseUrl: "https://api.kimi.com/coding/",
    websiteUrl: "https://www.kimi.com",
    model: "k3",
    tokenEnv: "POLARIS_STRESS_KIMI_TOKEN",
  },
];

function providerInput(provider, token) {
  const env = {
    ANTHROPIC_BASE_URL: provider.baseUrl,
    ANTHROPIC_AUTH_TOKEN: token,
    ANTHROPIC_MODEL: provider.model,
    ANTHROPIC_DEFAULT_HAIKU_MODEL: provider.model,
    ANTHROPIC_DEFAULT_SONNET_MODEL: provider.model,
    ANTHROPIC_DEFAULT_OPUS_MODEL: provider.model,
    CLAUDE_CODE_SUBAGENT_MODEL: provider.model,
  };
  return {
    id: provider.id,
    name: provider.name,
    note: "Isolated Polaris application-level capability stress test",
    websiteUrl: provider.websiteUrl,
    tokenField: "ANTHROPIC_AUTH_TOKEN",
    protocol: "",
    settingsConfig: { env },
  };
}

export async function configureProviders(client, credentials) {
  const missing = PROVIDERS.map((provider) => provider.tokenEnv).filter(
    (name) => typeof credentials?.[name] !== "string" || credentials[name].length === 0,
  );
  if (missing.length > 0) {
    throw new Error(`Missing required credential environment variables: ${missing.join(", ")}`);
  }

  for (const provider of PROVIDERS) {
    await client.invoke("provider_save", {
      input: providerInput(provider, credentials[provider.tokenEnv]),
    });
  }
  await client.invoke("provider_set_link_mode", { link: false });
  return PROVIDERS.map((provider) => ({ ...provider }));
}

function safeSegment(value, fallback) {
  const cleaned = String(value || "")
    .replace(/[^A-Za-z0-9._-]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 100);
  return cleaned || fallback;
}

async function prepareWorkDir(provider, scenario, options, attempt) {
  const runId = safeSegment(options.runId, "run");
  const providerId = safeSegment(provider.id, "provider");
  const scenarioId = safeSegment(scenario.id, "scenario");
  const workDir = path.resolve(
    options.runRoot,
    runId,
    providerId,
    scenarioId,
    `attempt-${attempt + 1}`,
  );
  await mkdir(workDir, { recursive: true });
  if (scenario.domain === "code") {
    await cp(CODE_FIXTURE, workDir, { recursive: true, force: true });
  }
  return workDir;
}

async function createConversation(client, provider, scenario, options, attempt) {
  const workDir = await prepareWorkDir(provider, scenario, options, attempt);
  const project = await client.invoke("conv_create_project", {
    name: `stress-${safeSegment(options.runId, "run")}-${provider.id}-${scenario.id}-a${attempt + 1}`,
  });
  if (!project?.id) throw new Error("Polaris conv_create_project returned no project id");
  await client.invoke("conv_set_project_work_dir", { projectId: project.id, workDir });
  const conversation = await client.invoke("conv_create_conversation", { projectId: project.id });
  if (!conversation?.id) throw new Error("Polaris conv_create_conversation returned no conversation id");
  return { workDir, projectId: project.id, conversationId: conversation.id };
}

async function executeTurn(client, chatArgs, timeoutMs) {
  const buffered = [];
  let state = null;
  let settle = null;
  let settleTimer = null;
  const terminal = new Promise((resolve) => {
    settle = resolve;
  });
  const onFrame = (event) => {
    if (!state) {
      buffered.push(event);
      return;
    }
    if (state.accept(event) && state.terminalCount > 0 && !settleTimer) {
      settleTimer = setTimeout(settle, 20);
    }
  };

  const disconnect = await client.connectEvents(onFrame);
  let requestId = "not-started";
  let timeoutHandle = null;
  try {
    try {
      requestId = await client.sendChat(chatArgs);
      state = new RunState({
        requestId,
        conversationId: chatArgs.conversationId,
        timeoutMs,
      });
      for (const event of buffered) onFrame(event);
    } catch (error) {
      return {
        requestId,
        conversationId: chatArgs.conversationId,
        terminalCount: 0,
        startedAt: Date.now(),
        firstEventAt: null,
        firstDeltaAt: null,
        finishedAt: Date.now(),
        inputTokens: null,
        text: "",
        tools: [],
        artifacts: [],
        errors: [error instanceof Error ? error.message : String(error)],
        timedOut: false,
      };
    }

    const timed = new Promise((resolve) => {
      timeoutHandle = setTimeout(() => {
        state.checkTimeout();
        resolve("timeout");
      }, timeoutMs);
    });
    const outcome = await Promise.race([terminal.then(() => "done"), timed]);
    if (outcome === "timeout") {
      try {
        await client.invoke("chat_cancel", { reqId: requestId });
      } catch {
        // Timeout evidence is already complete; cancellation is best effort.
      }
    }
    return state.result();
  } finally {
    clearTimeout(timeoutHandle);
    clearTimeout(settleTimer);
    disconnect();
  }
}

function mergeValidations(...results) {
  const checks = results.flatMap((result) => result?.checks || []);
  return { ok: checks.length > 0 && checks.every((check) => check.ok), checks };
}

function validateTurns(turns) {
  return mergeValidations(
    ...turns.map((turn, index) => ({
      ...validateGeneric(turn),
      checks: validateGeneric(turn).checks.map((check) => ({
        ...check,
        name: `turn-${index + 1}:${check.name}`,
      })),
    })),
  );
}

function manifestSnapshot(value, turn) {
  const units = Array.isArray(value?.units) ? value.units : [];
  const done = units.filter((unit) => unit?.status === "done").length;
  return {
    turn,
    valid: Array.isArray(value?.units),
    total: units.length,
    done,
    pending: Array.isArray(value?.units) ? units.length - done : -1,
  };
}

async function scanFiles(root, limit = 10_000) {
  const output = [];
  async function visit(directory) {
    if (output.length >= limit) return;
    let entries = [];
    try {
      entries = await readdir(directory, { withFileTypes: true });
    } catch {
      return;
    }
    for (const entry of entries) {
      if (output.length >= limit) break;
      const candidate = path.join(directory, entry.name);
      if (entry.isDirectory()) await visit(candidate);
      else if (entry.isFile()) output.push(candidate);
    }
  }
  await visit(root);
  return output;
}

async function artifactPaths(client, conversationId, workDir) {
  let entries = [];
  try {
    const result = await client.invoke("artifact_list", { conversationId });
    if (Array.isArray(result)) entries = result;
  } catch {
    // Work-directory scanning remains available when the artifact drawer cannot be read.
  }
  const workFiles = await scanFiles(workDir);
  return [...new Set([...entries.map((entry) => entry?.path).filter(Boolean), ...workFiles])];
}

async function readableFile(filePath) {
  try {
    return (await stat(filePath)).isFile();
  } catch {
    return false;
  }
}

async function validatePptArtifacts(paths, scenario) {
  const pptx = [];
  const html = [];
  const slideSources = [];
  for (const candidate of paths) {
    const lower = candidate.toLowerCase();
    if (!(await readableFile(candidate))) continue;
    if (lower.endsWith(".pptx")) pptx.push(candidate);
    if (lower.endsWith(".html") || lower.endsWith(".htm")) html.push(candidate);
    if (path.basename(lower) === "polaris.slides.json") slideSources.push(candidate);
  }

  const pptxResults = [];
  for (const candidate of pptx) {
    pptxResults.push({ path: candidate, result: await validatePptx(candidate, scenario.expectedSlides) });
  }
  const validDeck = pptxResults.find((item) => item.result.ok);
  const sourcePaths = [...slideSources, ...html];
  let sourceText = "";
  for (const sourcePath of sourcePaths) {
    try {
      const content = await readFile(sourcePath, "utf8");
      sourceText += `\n${content.slice(0, 16 * 1024 * 1024)}`;
    } catch {
      // Presence is checked separately; an unreadable source cannot satisfy required text.
    }
  }
  const missingText = (scenario.requiredText || []).filter((token) => !sourceText.includes(token));
  const checks = [
    {
      name: "pptx-artifact",
      ok: pptx.length > 0,
      detail: pptx.length > 0 ? `candidates=${pptx.length}` : "no PPTX artifact found",
    },
    {
      name: "pptx-structure",
      ok: Boolean(validDeck),
      detail: validDeck
        ? `${validDeck.path}: exact ${scenario.expectedSlides} slides`
        : pptxResults.map((item) => `${item.path}: ${item.result.checks[0]?.detail}`).join(" | ") ||
          "not inspected",
    },
    {
      name: "slides-source",
      ok: slideSources.length > 0,
      detail: `polaris.slides.json=${slideSources.length}`,
    },
    {
      name: "html-source",
      ok: html.length > 0,
      detail: `html=${html.length}`,
    },
    {
      name: "required-text",
      ok: missingText.length === 0,
      detail: missingText.length === 0 ? "all required text present" : `missing=${missingText.join(",")}`,
    },
  ];
  return { ok: checks.every((check) => check.ok), checks };
}

function evidenceText(turns, validation) {
  return [
    ...turns.flatMap((turn) => turn.errors || []),
    ...validation.checks.filter((check) => !check.ok).map((check) => `${check.name}: ${check.detail}`),
  ].join(" | ");
}

function classify(domain, turns, validation) {
  if (validation.ok) return { classification: "pass", retryable: false };
  const evidence = evidenceText(turns, validation);
  if (PROVIDER_RETRYABLE.test(evidence)) return { classification: "provider", retryable: true };
  if (/(?:authentication|unauthori[sz]ed|forbidden|invalid.*(?:token|key)|model.*not found|\b401\b|\b403\b)/i.test(evidence)) {
    return { classification: "provider", retryable: false };
  }
  if (/(?:single-terminal|within-timeout|timedOut=true|websocket|stream.*closed)/i.test(evidence)) {
    return { classification: "orchestration", retryable: false };
  }
  if (domain === "ppt" || /(?:pptx|artifact|slide)/i.test(evidence)) {
    return { classification: "artifact", retryable: false };
  }
  if (/(?:cloakbrowser|browser[-_ ]use|playwright|puppeteer|tool.*(?:missing|not found))/i.test(evidence)) {
    return { classification: "tool", retryable: false };
  }
  return { classification: "agent", retryable: false };
}

function metrics(turns, startedAt, finishedAt) {
  const firstEvents = turns.map((turn) => turn.firstEventAt).filter(Number.isFinite);
  const firstDeltas = turns.map((turn) => turn.firstDeltaAt).filter(Number.isFinite);
  return {
    durationMs: finishedAt - startedAt,
    firstEventMs: firstEvents.length ? Math.min(...firstEvents) - startedAt : null,
    firstDeltaMs: firstDeltas.length ? Math.min(...firstDeltas) - startedAt : null,
    toolCalls: turns.reduce((sum, turn) => sum + (turn.tools?.length || 0), 0),
    inputTokens: turns.reduce((sum, turn) => sum + (turn.inputTokens || 0), 0) || null,
    turns: turns.length,
  };
}

async function runAttempt(client, provider, scenario, options, attempt, batch) {
  const now = options.now || Date.now;
  const startedMs = now();
  const context = await createConversation(client, provider, scenario, options, attempt);
  const prompt = scenarioPrompt(scenario, {
    fixtureBaseUrl: options.fixtureBaseUrl,
    workDir: context.workDir,
  });
  const baseChatArgs = {
    permissionMode: "auto_all",
    useSandbox: false,
    skillIds: scenario.skillIds || [],
    conversationId: context.conversationId,
    dynamicWorkflow: false,
    useKb: false,
    batchBuild: Boolean(batch),
    batchSize: batch ? scenario.batchSize : null,
    agentMode: "single-agent",
    workMode: "work",
    providerId: provider.id,
  };
  const turns = [];
  const manifests = [];
  turns.push(await executeTurn(client, { ...baseChatArgs, prompt }, scenario.timeoutMs));

  if (batch) {
    let manifest = await client.invoke("chat_build_manifest", {
      conversationId: context.conversationId,
    });
    manifests.push(manifestSnapshot(manifest, turns.length));
    let previousPending = Number.MAX_SAFE_INTEGER;
    let stalls = 0;
    for (let index = 0; index < 40; index += 1) {
      const snapshot = manifests.at(-1);
      if (snapshot.valid && snapshot.total > 0 && snapshot.pending === 0) break;
      if (snapshot.pending < 0 || snapshot.pending >= previousPending) stalls += 1;
      else stalls = 0;
      if (stalls >= 3) break;
      if (snapshot.pending >= 0) previousPending = snapshot.pending;
      turns.push(
        await executeTurn(
          client,
          { ...baseChatArgs, prompt: CONTINUE_PROMPT },
          scenario.timeoutMs,
        ),
      );
      manifest = await client.invoke("chat_build_manifest", {
        conversationId: context.conversationId,
      });
      manifests.push(manifestSnapshot(manifest, turns.length));
    }
  }

  const paths = await artifactPaths(client, context.conversationId, context.workDir);
  let validation;
  if (scenario.domain === "browser") {
    validation = validateBrowser(turns[0], scenario);
  } else if (scenario.domain === "code") {
    validation = mergeValidations(validateTurns(turns), await validateCode(context.workDir, scenario));
  } else if (scenario.domain === "ppt") {
    validation = mergeValidations(validateTurns(turns), await validatePptArtifacts(paths, scenario));
  } else {
    validation = validateTurns(turns);
  }
  const finishedMs = now();
  const classified = classify(scenario.domain, turns, validation);
  return {
    runId: options.runId,
    providerId: provider.id,
    scenarioId: scenario.id,
    domain: scenario.domain,
    startedAt: new Date(startedMs).toISOString(),
    finishedAt: new Date(finishedMs).toISOString(),
    metrics: metrics(turns, startedMs, finishedMs),
    validation,
    classification: classified.classification,
    retryable: classified.retryable,
    retryCount: attempt,
    manifests,
    projectId: context.projectId,
    conversationId: context.conversationId,
    workDir: context.workDir,
    artifacts: paths,
    error: turns.flatMap((turn) => turn.errors || []).join(" | ") || undefined,
  };
}

async function runWithRetries(client, provider, scenario, options, batch) {
  const maxRetries = Math.max(0, options.maxRetries ?? 1);
  const sleep = options.sleep || ((milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds)));
  const attempts = [];
  for (let attempt = 0; attempt <= maxRetries; attempt += 1) {
    const record = await runAttempt(client, provider, scenario, options, attempt, batch);
    attempts.push({ classification: record.classification, validationOk: record.validation.ok });
    if (!record.retryable || attempt === maxRetries) {
      return { ...record, retryCount: attempt, previousAttempts: attempts.slice(0, -1) };
    }
    await sleep(1_000 * 2 ** attempt);
  }
  throw new Error("unreachable retry state");
}

export function runScenario(client, provider, scenario, options) {
  return runWithRetries(client, provider, scenario, options, Boolean(scenario.batchBuild));
}

export function runBatchScenario(client, provider, scenario, options) {
  return runWithRetries(client, provider, scenario, options, true);
}

export function parseHarnessArgs(argv) {
  const [command, ...rest] = argv;
  if (!KNOWN_COMMANDS.has(command)) throw new Error(`Unknown command: ${command || "(missing)"}`);
  const result = {
    command,
    baseUrl: "http://127.0.0.1:8899",
    providers: PROVIDERS.map((provider) => provider.id),
    scenarioIds: [],
    concurrency: null,
  };
  for (let index = 0; index < rest.length; index += 1) {
    const flag = rest[index];
    if (CREDENTIAL_FLAG.test(flag)) {
      throw new Error("Credentials must be supplied through environment variables, never CLI flags");
    }
    const value = rest[index + 1];
    if (!flag.startsWith("--") || value == null || value.startsWith("--")) {
      throw new Error(`Invalid argument: ${flag}`);
    }
    if (flag === "--base-url") result.baseUrl = value.replace(/\/$/, "");
    else if (flag === "--providers") result.providers = value.split(",").filter(Boolean);
    else if (flag === "--scenarios") result.scenarioIds = value.split(",").filter(Boolean);
    else if (flag === "--concurrency") result.concurrency = Number.parseInt(value, 10);
    else throw new Error(`Unknown option: ${flag}`);
    index += 1;
  }
  const base = new URL(result.baseUrl);
  if (!new Set(["127.0.0.1", "localhost", "[::1]", "::1"]).has(base.hostname)) {
    throw new Error("Polaris base URL must resolve to loopback");
  }
  return result;
}

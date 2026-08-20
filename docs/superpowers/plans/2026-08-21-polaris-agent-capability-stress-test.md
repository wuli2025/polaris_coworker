# Polaris Agent Capability Stress Test Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build and run a repeatable application-level harness that makes Polaris Agent complete browser, coding, and long PPT tasks through the real `chat_send` pipeline on MiniMax and Kimi K3, then fix reproducible Polaris defects and publish a redacted evidence report.

**Architecture:** A dependency-free Node.js controller talks only to Polaris `/api/invoke` and `/ws`, creates isolated projects and conversations, and validates the files Polaris Agent produces. Deterministic browser and code fixtures make correctness measurable; PPTX inspection uses a read-only Python standard-library validator. Production fixes are admitted only after an application-level failure is reproduced and a focused regression test fails.

**Tech Stack:** Node.js 22 ESM (`fetch`, `WebSocket`, `node:test`-compatible modules), Vitest 3, Python 3 `zipfile`, Polaris axum HTTP/WebSocket data plane, Vue 3/Pinia, Rust/Cargo.

**Spec:** `docs/superpowers/specs/2026-08-21-polaris-agent-capability-stress-test-design.md`

## Global Constraints

- Every measured task enters through Polaris `chat_send`; the controller must never call MiniMax, Kimi, or Claude Code directly.
- The controller must never perform browser research, implement fixture code changes, or generate presentation content on behalf of Polaris Agent.
- MiniMax uses `https://api.minimaxi.com/anthropic` with model `MiniMax-M3`; Kimi uses Anthropic Base URL `https://api.kimi.com/coding/` with model `k3`.
- Secrets may exist only in runtime provider configuration or process input and must be redacted from console output, JSONL, Markdown, snapshots, and test failures.
- Browser tasks are read-only; side-effect cases target only the local fixture site.
- Coding tasks may modify only run-specific copies under `_agent_stress/work/`.
- Existing uncommitted workspace changes must not be reset, overwritten, or included in unrelated commits.
- Provider rate limits trigger bounded backoff and load reduction; they are not classified as Polaris defects.

---

## File Structure

- `scripts/agent-stress/redact.mjs`: recursive secret and credential redaction.
- `scripts/agent-stress/client.mjs`: Polaris invoke and WebSocket event client.
- `scripts/agent-stress/run-state.mjs`: per-request event state machine, timeout, cancellation, and metrics.
- `scripts/agent-stress/scenarios.mjs`: browser, code, and PPT task definitions.
- `scripts/agent-stress/fixture-server.mjs`: deterministic local dynamic website used by Polaris Agent.
- `scripts/agent-stress/fixtures/code/`: immutable small repositories copied for coding scenarios.
- `scripts/agent-stress/validators.mjs`: generic, browser, code, and artifact validation.
- `scripts/agent-stress/validate-pptx.py`: read-only OOXML structure and slide-count validation.
- `scripts/agent-stress/orchestrator.mjs`: provider setup, project/session creation, staged concurrency, and long-task continuation.
- `scripts/agent-stress/report.mjs`: JSONL aggregation and Markdown summary.
- `scripts/agent-stress/*.test.mjs`: focused unit tests for controller behavior.
- `_agent_stress/results/`: ignored runtime evidence and redacted reports.
- `.gitignore`: ignore run workspaces, raw events, and generated artifacts.
- `package.json`: add `test:agent-stress` and `stress:agent` commands.

### Task 1: Safe Polaris Application Client

**Files:**
- Create: `scripts/agent-stress/redact.mjs`
- Create: `scripts/agent-stress/client.mjs`
- Create: `scripts/agent-stress/run-state.mjs`
- Test: `scripts/agent-stress/client.test.mjs`
- Test: `scripts/agent-stress/run-state.test.mjs`

**Interfaces:**
- Produces: `redact(value, secrets): unknown`
- Produces: `new PolarisClient({ baseUrl, token, fetchImpl, WebSocketImpl })`
- Produces: `client.invoke(cmd, args): Promise<unknown>`
- Produces: `client.connectEvents(onFrame): Promise<() => void>`
- Produces: `new RunState({ requestId, conversationId, timeoutMs, now })`
- Produces: `runState.accept(frame): void`, `runState.result(): RunMetrics`
- `RunMetrics = { requestId, conversationId, terminalCount, startedAt, firstEventAt, firstDeltaAt, finishedAt, text, tools, artifacts, errors }`

- [ ] **Step 1: Write redaction and HTTP contract tests**

```js
it("redacts secrets recursively without changing metric fields", () => {
  const input = { token: "secret-A", nested: ["prefix secret-B suffix"], latencyMs: 42 };
  expect(redact(input, ["secret-A", "secret-B"])).toEqual({
    token: "[REDACTED]",
    nested: ["prefix [REDACTED] suffix"],
    latencyMs: 42,
  });
});

it("wraps chat_send arguments exactly once", async () => {
  const client = fakeClient();
  await client.sendChat({ prompt: "probe", conversationId: "c-1" });
  expect(client.lastBody).toEqual({ cmd: "chat_send", args: { args: expect.any(Object) } });
});
```

- [ ] **Step 2: Run the client tests and verify the expected missing-module failure**

Run: `npx vitest run scripts/agent-stress/client.test.mjs`

Expected: FAIL because `redact.mjs` and `client.mjs` do not exist.

- [ ] **Step 3: Implement recursive redaction and the application-only client**

```js
export class PolarisClient {
  constructor({ baseUrl, token = "", fetchImpl = fetch, WebSocketImpl = WebSocket }) {
    this.baseUrl = baseUrl.replace(/\/$/, "");
    this.token = token;
    this.fetchImpl = fetchImpl;
    this.WebSocketImpl = WebSocketImpl;
  }

  async invoke(cmd, args = {}) {
    const response = await this.fetchImpl(`${this.baseUrl}/api/invoke`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        ...(this.token ? { authorization: `Bearer ${this.token}` } : {}),
      },
      body: JSON.stringify({ cmd, args }),
    });
    const body = await response.json();
    if (!response.ok) throw new Error(body.error || `HTTP ${response.status}`);
    return body;
  }

  sendChat(args) {
    return this.invoke("chat_send", { args });
  }
}
```

- [ ] **Step 4: Write state-machine tests for event isolation and one terminal state**

```js
it("ignores another conversation and closes on its own done frame", () => {
  const state = new RunState({ requestId: "r1", conversationId: "c1", timeoutMs: 1000, now: clock() });
  state.accept(frame("r2", "c2", "delta", "wrong"));
  state.accept(frame("r1", "c1", "tool", "Read"));
  state.accept(frame("r1", "c1", "done"));
  expect(state.result()).toMatchObject({ terminalCount: 1, tools: ["Read"], text: "" });
});
```

- [ ] **Step 5: Run both focused tests and the existing frontend unit suite**

Run: `npx vitest run scripts/agent-stress/client.test.mjs scripts/agent-stress/run-state.test.mjs`

Expected: PASS.

Run: `npm run test:unit`

Expected: existing suite remains green.

- [ ] **Step 6: Commit the safe client**

```bash
git add scripts/agent-stress/redact.mjs scripts/agent-stress/client.mjs scripts/agent-stress/run-state.mjs scripts/agent-stress/client.test.mjs scripts/agent-stress/run-state.test.mjs
git commit -m "test: add Polaris agent stress client"
```

### Task 2: Deterministic Browser and Code Fixtures

**Files:**
- Create: `scripts/agent-stress/fixture-server.mjs`
- Create: `scripts/agent-stress/scenarios.mjs`
- Create: `scripts/agent-stress/fixtures/code/order-service/package.json`
- Create: `scripts/agent-stress/fixtures/code/order-service/src/pricing.mjs`
- Create: `scripts/agent-stress/fixtures/code/order-service/src/orders.mjs`
- Create: `scripts/agent-stress/fixtures/code/order-service/test/pricing.test.mjs`
- Create: `scripts/agent-stress/fixtures/code/order-service/test/orders.test.mjs`
- Test: `scripts/agent-stress/scenarios.test.mjs`

**Interfaces:**
- Produces: `startFixtureServer({ host, port }): Promise<{ baseUrl, close }>`
- Produces: `SCENARIOS: Scenario[]`
- `Scenario = { id, domain, prompt, skillIds, timeoutMs, batchBuild, batchSize, validator }`

- [ ] **Step 1: Write scenario safety and coverage tests**

```js
it("covers every domain and keeps coding work inside the run directory", () => {
  expect(new Set(SCENARIOS.map((s) => s.domain))).toEqual(new Set(["browser", "code", "ppt"]));
  expect(SCENARIOS.filter((s) => s.domain === "browser")).toHaveLength(8);
  expect(SCENARIOS.filter((s) => s.domain === "code")).toHaveLength(8);
  expect(SCENARIOS.filter((s) => s.domain === "ppt")).toHaveLength(6);
  expect(SCENARIOS.every((s) => !/登录真实|购买|发布|发送消息/.test(s.prompt))).toBe(true);
});
```

- [ ] **Step 2: Run the scenario test and verify it fails because scenarios are absent**

Run: `npx vitest run scripts/agent-stress/scenarios.test.mjs`

Expected: FAIL with module-not-found.

- [ ] **Step 3: Implement the deterministic website**

The fixture server must expose these exact routes:

```text
GET /catalog                 dynamic shell; JavaScript fetches /api/items?page=1
GET /api/items?page=1|2|3    paginated products with stable ids and prices
GET /detail/:id              product detail with a unique compliance code
GET /report.csv              downloadable UTF-8 CSV
GET /slow?ms=1500            delayed response used for timeout behavior
POST /form                   accepts only marker STRESS-ONLY and stores nothing
```

- [ ] **Step 4: Add eight browser scenarios, eight code scenarios, and six PPT scenarios**

Browser prompts must explicitly require Polaris to use the browser, report the visited URL, and quote deterministic fields. Code prompts must ask Polaris to change only the copied fixture and finish by running `npm test`. PPT prompts must request `polaris.slides.json`, HTML source, and PPTX under the bound run directory; PPT sizes are 12, 30, 60, 12, 30, and 60 slides.

- [ ] **Step 5: Add the intentionally failing code fixture**

```js
export function discountedTotal(items, percent) {
  const subtotal = items.reduce((sum, item) => sum + item.price * item.quantity, 0);
  return Math.round(subtotal * (1 - percent)); // fixture defect: percent is an integer such as 15
}
```

The fixture tests assert a 15 percent discount, invalid quantities, stable order sorting, and no input mutation. Different scenarios request independent fixes or features in fresh copies.

- [ ] **Step 6: Run fixture and scenario tests**

Run: `npx vitest run scripts/agent-stress/scenarios.test.mjs`

Expected: PASS.

Run: `node --test scripts/agent-stress/fixtures/code/order-service/test/*.test.mjs`

Expected: FAIL only on the deliberately broken behavior, proving the coding fixture can detect an Agent fix.

- [ ] **Step 7: Commit fixtures and scenarios**

```bash
git add scripts/agent-stress/fixture-server.mjs scripts/agent-stress/scenarios.mjs scripts/agent-stress/scenarios.test.mjs scripts/agent-stress/fixtures/code
git commit -m "test: add deterministic agent capability scenarios"
```

### Task 3: Artifact and Outcome Validators

**Files:**
- Create: `scripts/agent-stress/validators.mjs`
- Create: `scripts/agent-stress/validate-pptx.py`
- Test: `scripts/agent-stress/validators.test.mjs`

**Interfaces:**
- Produces: `validateGeneric(run): ValidationResult`
- Produces: `validateBrowser(run, scenario): ValidationResult`
- Produces: `validateCode(runDir, scenario): Promise<ValidationResult>`
- Produces: `validatePptx(path, expectedSlides): Promise<ValidationResult>`
- `ValidationResult = { ok: boolean, checks: { name, ok, detail }[] }`

- [ ] **Step 1: Write validator tests using valid and corrupt temporary artifacts**

```js
it("rejects a run without exactly one done event", () => {
  expect(validateGeneric({ terminalCount: 0, errors: [], artifacts: [] }).ok).toBe(false);
});

it("rejects a renamed text file pretending to be pptx", async () => {
  const path = await tempFile("fake.pptx", "not a zip");
  expect((await validatePptx(path, 12)).ok).toBe(false);
});
```

- [ ] **Step 2: Run validators test and verify missing implementation failure**

Run: `npx vitest run scripts/agent-stress/validators.test.mjs`

Expected: FAIL with module-not-found.

- [ ] **Step 3: Implement read-only PPTX inspection**

```python
with zipfile.ZipFile(path) as deck:
    names = set(deck.namelist())
    slides = sorted(n for n in names if re.fullmatch(r"ppt/slides/slide\d+\.xml", n))
    required = {"[Content_Types].xml", "ppt/presentation.xml"}
    broken = sorted(required - names)
    print(json.dumps({"ok": not broken and len(slides) == expected,
                      "slides": len(slides), "missing": broken}, ensure_ascii=False))
```

- [ ] **Step 4: Implement code and browser validators**

`validateCode` runs the scenario's declared deterministic test command inside the copied fixture and records exit code/stdout tail. `validateBrowser` requires at least one browser-related tool event plus every scenario `expectedToken`; an answer with tokens but no browser tool evidence is marked hallucinated, not passed.

- [ ] **Step 5: Run validator tests**

Run: `npx vitest run scripts/agent-stress/validators.test.mjs`

Expected: PASS.

- [ ] **Step 6: Commit validators**

```bash
git add scripts/agent-stress/validators.mjs scripts/agent-stress/validate-pptx.py scripts/agent-stress/validators.test.mjs
git commit -m "test: validate Polaris agent outcomes"
```

### Task 4: Provider-Isolated Orchestrator and Report

**Files:**
- Create: `scripts/agent-stress/orchestrator.mjs`
- Create: `scripts/agent-stress/report.mjs`
- Create: `scripts/agent-stress/run.mjs`
- Test: `scripts/agent-stress/orchestrator.test.mjs`
- Test: `scripts/agent-stress/report.test.mjs`
- Modify: `.gitignore`
- Modify: `package.json`

**Interfaces:**
- Consumes: `PolarisClient`, `RunState`, `SCENARIOS`, validators.
- Produces: `configureProviders(client, credentials): Promise<ProviderConfig[]>`
- Produces: `runScenario(client, provider, scenario, options): Promise<RunRecord>`
- Produces: `runBatchScenario(client, provider, scenario, options): Promise<RunRecord>`
- Produces: `writeReport(records, outputDir): Promise<{ jsonlPath, markdownPath }>`
- `ProviderConfig = { id, name, baseUrl, model, tokenEnv }`, where `tokenEnv` is the environment-variable name and never the secret value.
- `RunRecord = { runId, providerId, scenarioId, domain, startedAt, finishedAt, metrics, validation, classification, retryCount, manifests }`

- [ ] **Step 1: Write tests that forbid direct model traffic and verify provider payloads**

```js
it("configures Kimi Anthropic mode without the OpenAI /v1 base", async () => {
  const calls = [];
  await configureProviders(fakeInvoke(calls), credentials());
  const kimi = calls.find((c) => c.args.input.id === "stress-kimi-k3");
  expect(kimi.args.input.protocol).toBe("");
  expect(kimi.args.input.settingsConfig.env.ANTHROPIC_BASE_URL).toBe("https://api.kimi.com/coding/");
  expect(kimi.args.input.settingsConfig.env.ANTHROPIC_MODEL).toBe("k3");
});

it("backs off after 429 and never fetches a provider hostname", async () => {
  const seen = [];
  await runWithFetchGuard((url) => seen.push(new URL(url).hostname));
  expect(new Set(seen)).toEqual(new Set(["127.0.0.1", "localhost"]));
});
```

- [ ] **Step 2: Run orchestrator tests and verify the expected missing-module failure**

Run: `npx vitest run scripts/agent-stress/orchestrator.test.mjs scripts/agent-stress/report.test.mjs`

Expected: FAIL because orchestrator/report modules do not exist.

- [ ] **Step 3: Implement provider setup through Polaris**

Each provider uses `provider_save` with all four Anthropic model aliases plus `CLAUDE_CODE_SUBAGENT_MODEL`, followed by `provider_set_link_mode(false)` and per-conversation `providerId`. Do not log provider input. Preserve the pre-run current provider id and restore it in `finally`.

Credentials are read only from `POLARIS_STRESS_MINIMAX_TOKEN` and `POLARIS_STRESS_KIMI_TOKEN`; the CLI must reject token flags so shell history and process listings cannot contain keys.

- [ ] **Step 4: Implement isolated project and conversation creation**

For every scenario, copy its fixture into `_agent_stress/work/$STRESS_RUN_ID/$STRESS_PROVIDER_ID/$STRESS_SCENARIO_ID/`, then call `conv_create_project`, `conv_set_project_work_dir`, and `conv_create_conversation`. Submit with `permissionMode: "auto_all"`, `workMode: "work"`, explicit `providerId`, and scenario skill ids. These three variables are generated by the orchestrator from the run timestamp and the selected scenario record; they are never accepted from model output.

- [ ] **Step 5: Implement long-task continuation**

After every `done`, call `chat_build_manifest`. Continue with the exact application prompt used by `src/stores/longtask.ts`, stop at all-done, three non-progress turns, or 40 turns, and record manifest counts after every turn. Every continuation is another Polaris `chat_send`; the controller never creates slide content.

- [ ] **Step 6: Implement staged load and reporting**

Support commands `preflight`, `matrix`, `concurrency`, `recovery`, `replay`, `replay-actionable`, and `all`. `replay-actionable` reads `_agent_stress/results/latest/events.jsonl`, selects the highest-severity non-provider failure, and replays its recorded scenario/provider pair without shell interpolation. Concurrency advances through 2, 4, and 8 only when the previous level has no service-wide failure. JSONL is written after each completed scenario so interruption does not lose evidence.

- [ ] **Step 7: Add scripts and ignore runtime data**

```json
{
  "scripts": {
    "test:agent-stress": "vitest run scripts/agent-stress/*.test.mjs",
    "stress:agent": "node scripts/agent-stress/run.mjs"
  }
}
```

Append `/_agent_stress/` to `.gitignore`.

- [ ] **Step 8: Run focused and full harness unit tests**

Run: `npm run test:agent-stress`

Expected: PASS with no secret-like value in snapshots or output.

Run: `npm run test:unit`

Expected: existing frontend tests and new harness tests pass.

- [ ] **Step 9: Commit orchestrator and report generator**

```bash
git add scripts/agent-stress/orchestrator.mjs scripts/agent-stress/report.mjs scripts/agent-stress/run.mjs scripts/agent-stress/orchestrator.test.mjs scripts/agent-stress/report.test.mjs .gitignore package.json
git commit -m "test: orchestrate Polaris agent capability runs"
```

### Task 5: Application Preflight and Baseline Capability Matrix

**Files:**
- Runtime output: `_agent_stress/results/latest/events.jsonl`
- Runtime output: `_agent_stress/results/latest/report.md`
- Possible production tests: exact module selected from failure evidence.

**Interfaces:**
- Consumes: `npm run stress:agent -- preflight|matrix|concurrency|recovery|replay|replay-actionable|all` and only Polaris HTTP/WebSocket endpoints.
- Produces: redacted baseline evidence for both providers.

- [ ] **Step 1: Build the current Polaris server application**

Run: `cargo build -p polaris-cli --bin polaris-server --release`

Expected: exit 0 and `src-tauri/target/release/polaris-server` exists.

- [ ] **Step 2: Start Polaris on a dedicated port and verify health**

Run the built application with `HOME=$PWD/_agent_stress/home`, `POLARIS_PORT=8899`, and `POLARIS_STRICT_ARGS=1`. The isolated home prevents provider, conversation, skill, and artifact mutations from reaching the user's normal Polaris data. Test credentials are supplied only to the harness process through the two required environment variables. Verify `GET http://127.0.0.1:8899/api/health` returns `ok`.

- [ ] **Step 3: Run application preflight**

Run: `npm run stress:agent -- preflight --base-url http://127.0.0.1:8899`

Expected: for each provider, a short answer, a real tool call, and a small artifact reach exactly one `done` state. Any failure blocks load escalation and enters Task 6.

- [ ] **Step 4: Run the serial matrix**

Run: `npm run stress:agent -- matrix --base-url http://127.0.0.1:8899 --providers stress-minimax-m3,stress-kimi-k3`

Expected: 44 provider-scenario runs complete or receive an explicit classified failure; no run remains pending.

- [ ] **Step 5: Review failure clusters before modifying production code**

Run: `node scripts/agent-stress/report.mjs _agent_stress/results/latest/events.jsonl`

Expected: failures grouped into `provider`, `agent`, `tool`, `orchestration`, `artifact`, or `ux`, with provider responses separated from Polaris defects.

### Task 6: Evidence-Driven Polaris Fixes

**Files:**
- Modify only the production module identified by a reproduced failure.
- Create or modify the nearest existing Rust/Vitest test module for that behavior.
- Runtime output: `_agent_stress/results/latest/fix-verification.jsonl`

**Interfaces:**
- Consumes: one minimal failing scenario id and its redacted event trace.
- Produces: one regression test plus one focused production fix per defect.

- [ ] **Step 1: Select the highest-severity reproducible Polaris failure**

Choose in order: service crash or permanent sending state; conversation/event cross-talk; lost/corrupt artifact; long-task stall; browser/tool unavailability without actionable error; misleading UX. Upstream authentication, 429, and 5xx are excluded unless Polaris mishandles them.

- [ ] **Step 2: Re-run only that scenario twice through Polaris**

Run: `npm run stress:agent -- replay-actionable --repeat 2 --base-url http://127.0.0.1:8899`

Expected: the same application-layer symptom occurs twice. If it does not, retain it as flaky evidence and do not patch production code.

- [ ] **Step 3: Use systematic debugging to trace the failing layer**

Read the full error, compare the equivalent passing provider/scenario, and trace `invoke -> chat pipeline -> session/tool -> event -> validator`. State one root-cause hypothesis and test it with the smallest diagnostic change.

- [ ] **Step 4: Add the smallest failing regression test**

Create the regression in one of these fixed locations: backend chat/session behavior uses the inline `#[cfg(test)]` module in `src-tauri/crates/polaris-kernel/src/chat/pipeline.rs` or `session_pool.rs`; frontend terminal/long-task behavior uses `src/stores/chat.test.ts` or `src/stores/longtask.test.ts`; controller state behavior uses the existing stress-harness tests. Run the focused command for the classified layer:

```bash
cargo test -p polaris-kernel -- --nocapture
npx vitest run src/stores/chat.test.ts src/stores/longtask.test.ts
npx vitest run scripts/agent-stress/run-state.test.mjs scripts/agent-stress/orchestrator.test.mjs
```

Expected: FAIL for the reproduced application behavior, not because of setup or syntax.

- [ ] **Step 5: Implement one root-cause fix and verify green**

Run the same focused test and then the affected crate/store suite. Expected: PASS with no new warnings attributable to the change.

- [ ] **Step 6: Rebuild and replay through Polaris**

Run: `cargo build -p polaris-cli --bin polaris-server --release`

Run: `npm run stress:agent -- replay-actionable --repeat 2 --base-url http://127.0.0.1:8899`

Expected: both application-level replays pass.

- [ ] **Step 7: Commit the isolated fix**

Use `git diff --name-only` to identify the one production file and one regression-test file changed in this fix, stage those two literal paths only, then run `git commit -m "fix: improve Polaris agent recovery"`. If `git diff --name-only` lists any pre-existing user file not intentionally changed by this fix, leave it unstaged.

- [ ] **Step 8: Repeat Tasks 6.1–6.7 for each remaining P0/P1 defect**

Stop after three failed fix hypotheses for the same defect and escalate it as an architectural issue in the report instead of stacking a fourth speculative patch.

### Task 7: Concurrency, Recovery, and Long-PPT Verification

**Files:**
- Runtime output: `_agent_stress/results/latest/events.jsonl`
- Runtime output: `_agent_stress/results/latest/report.md`
- Modify production/test files only if Task 6 admission rules are met.

**Interfaces:**
- Consumes: repaired Polaris build and passing serial scenarios.
- Produces: load-level metrics, recovery evidence, and a validated 60-slide artifact for each provider.

- [ ] **Step 1: Run concurrency level 2**

Run: `npm run stress:agent -- concurrency --levels 2 --base-url http://127.0.0.1:8899`

Expected: no event cross-talk, service crash, or permanently active request.

- [ ] **Step 2: Advance to levels 4 and 8 conditionally**

Run: `npm run stress:agent -- concurrency --levels 4,8 --base-url http://127.0.0.1:8899`

Expected: each level runs only after the prior level's application health gate passes. 429 causes provider-specific backoff and does not abort the other provider.

- [ ] **Step 3: Run cancellation and recovery cases**

Run: `npm run stress:agent -- recovery --base-url http://127.0.0.1:8899`

Expected: cancellation removes the active request, emits or synthesizes one terminal state, and a new conversation can complete afterward.

- [ ] **Step 4: Complete one 60-slide deck per provider**

Run: `npm run stress:agent -- replay --scenario ppt-60-long --provider stress-minimax-m3 --base-url http://127.0.0.1:8899`

Run: `npm run stress:agent -- replay --scenario ppt-60-long --provider stress-kimi-k3 --base-url http://127.0.0.1:8899`

Expected: manifest pending count decreases monotonically to zero and the validator reports exactly 60 slides with no missing required OOXML parts.

- [ ] **Step 5: Apply Task 6 to any newly reproducible P0/P1 application defect**

Do not patch failures attributable solely to exhausted membership, provider 429, or provider outage.

### Task 8: Fresh Verification and Final Report

**Files:**
- Create: `docs/reports/2026-08-21-polaris-agent-capability-stress-test.md`
- Modify: `README.md` only if a new supported stress command needs operator documentation.

**Interfaces:**
- Consumes: all redacted run records and committed fixes.
- Produces: final comparison, defect list, evidence, and remaining limitations.

- [ ] **Step 1: Run the complete harness unit suite**

Run: `npm run test:agent-stress`

Expected: PASS.

- [ ] **Step 2: Run frontend, boundary, and production build checks**

Run: `npm run test:unit`

Run: `npm run check:boundaries`

Run: `npm run build`

Expected: all exit 0.

- [ ] **Step 3: Run affected Rust suites and server build**

Run: `cargo test -p polaris-runtime -p polaris-kernel -p polaris-forge -p polaris-watchdog`

Run: `cargo test -p polaris-app --no-default-features --features server`

Run: `cargo build -p polaris-cli --bin polaris-server --release`

Expected: all exit 0.

- [ ] **Step 4: Run a final two-provider smoke through Polaris**

Run: `npm run stress:agent -- preflight --base-url http://127.0.0.1:8899`

Expected: both providers finish browser/tool/artifact smoke with exactly one terminal state.

- [ ] **Step 5: Generate and inspect the final report**

The report must include scenario counts, pass rates, p50/p95 first-event and total durations, tool-use evidence, 12/30/60-slide validation, code-test results, concurrency level reached, every production fix with before/after evidence, upstream limitations, and explicit confirmation that no secret appears.

- [ ] **Step 6: Scan tracked and report files for exposed credentials**

Run: `git grep -nE 'sk-[A-Za-z0-9_-]{16,}|ANTHROPIC_AUTH_TOKEN[^[:space:]]*[=:][^[:space:]]+' -- ':!package-lock.json'`

Expected: no newly introduced credential value.

- [ ] **Step 7: Commit the final report**

```bash
git add docs/reports/2026-08-21-polaris-agent-capability-stress-test.md README.md
git commit -m "docs: report Polaris agent capability stress results"
```

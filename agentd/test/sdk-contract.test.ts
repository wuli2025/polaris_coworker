// Pins the slice of the Pi SDK that agentd depends on. If a Pi upgrade breaks
// any of these behaviours, this file fails before agentd code does.
import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import { fauxAssistantMessage, fauxToolCall, InMemoryCredentialStore } from "@earendil-works/pi-ai";
import { registerFauxProvider } from "@earendil-works/pi-ai/compat";
import {
	type AgentSessionEvent,
	createAgentSession,
	defineTool,
	ModelRuntime,
	SessionManager,
	SettingsManager,
} from "@earendil-works/pi-coding-agent";
import { Type } from "typebox";

import { bounded } from "./synchronization.ts";

const FAUX_KEY = "faux-key";
const TOOL_NAME = "polaris_echo";

async function fauxSession(responses: Parameters<ReturnType<typeof registerFauxProvider>["setResponses"]>[0]) {
	const dir = mkdtempSync(join(tmpdir(), "agentd-contract-"));
	const faux = registerFauxProvider();
	faux.setResponses(responses);
	const model = faux.getModel();
	const credentials = new InMemoryCredentialStore();
	await credentials.modify(model.provider, async () => ({ type: "api_key", key: FAUX_KEY }));
	const modelRuntime = await ModelRuntime.create({ credentials, modelsPath: null, allowModelNetwork: false });
	const { id, name, api, reasoning, input, cost, contextWindow, maxTokens, baseUrl } = model;
	const models = [{ id, name, api, reasoning, input, cost, contextWindow, maxTokens, baseUrl }];
	modelRuntime.registerProvider(model.provider, { baseUrl, api, models });
	const toolCalls: unknown[] = [];
	const echo = defineTool({
		name: TOOL_NAME,
		label: "Echo",
		description: "Echo the given text back.",
		parameters: Type.Object({ text: Type.String() }),
		execute: async (_id, params) => {
			toolCalls.push(params);
			return { content: [{ type: "text", text: `echo:${params.text}` }], details: {} };
		},
	});
	const { session } = await createAgentSession({
		cwd: dir,
		agentDir: dir,
		model,
		modelRuntime,
		// `tools` is an allowlist over *all* tools: `tools: []` silently disables custom
		// tools too. `noTools: "builtin"` drops only read/bash/edit/write.
		noTools: "builtin",
		customTools: [echo],
		sessionManager: SessionManager.inMemory(dir),
		settingsManager: SettingsManager.inMemory({ compaction: { enabled: false } }),
	});
	const events: AgentSessionEvent[] = [];
	session.subscribe((event) => events.push(event));
	const close = () => {
		session.dispose();
		faux.unregister();
		rmSync(dir, { recursive: true, force: true });
	};
	return { session, events, toolCalls, close };
}

test("prompt runs a custom tool and streams the final answer", async () => {
	const { session, events, toolCalls, close } = await fauxSession([
		fauxAssistantMessage(fauxToolCall(TOOL_NAME, { text: "hi" }), { stopReason: "toolUse" }),
		fauxAssistantMessage("done"),
	]);
	try {
		await session.prompt("use the tool");
		assert.deepEqual(toolCalls, [{ text: "hi" }]);
		assert.equal(session.getLastAssistantText(), "done");
		const types = new Set(events.map((event) => event.type));
		for (const expected of ["agent_start", "tool_execution_start", "tool_execution_end", "message_end", "agent_end"]) {
			assert.ok(types.has(expected as AgentSessionEvent["type"]), `missing event ${expected}`);
		}
	} finally {
		close();
	}
});

test("abort stops a running turn and leaves the session usable", async () => {
	let entered!: () => void;
	const started = new Promise<void>((resolve) => { entered = resolve; });
	const { session, close } = await fauxSession([
		(_context, options) => new Promise((resolve) => {
			const abort = () => resolve(fauxAssistantMessage("", { stopReason: "aborted" }));
			options?.signal?.addEventListener("abort", abort, { once: true });
			if (options?.signal?.aborted) abort();
			entered();
		}),
		fauxAssistantMessage("after abort"),
	]);
	try {
		const running = session.prompt("slow").catch(() => undefined);
		await bounded(started, "SDK model entered");
		await session.abort();
		await running;
		await session.prompt("again");
		assert.equal(session.getLastAssistantText(), "after abort");
	} finally {
		close();
	}
});

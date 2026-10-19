import assert from "node:assert/strict";
import { test } from "node:test";
import { fauxAssistantMessage, type AssistantMessage } from "@earendil-works/pi-ai";
import { EventMapper } from "../src/events/mapper.ts";
import type { ProtocolEvent } from "../src/protocol/events.ts";

type Diagnostic = NonNullable<AssistantMessage["diagnostics"]>[number];
const cases: [string, Partial<Diagnostic>, string][] = [
  ["HTTP 401", { details: { status: 401 } }, "MODEL_AUTH"],
  ["HTTP 403", { details: { status: 403 } }, "MODEL_AUTH"],
  ["auth type", { details: { errorType: "authentication_error" } }, "MODEL_AUTH"],
  ["Bedrock auth", { details: { errorCode: "AccessDeniedException" } }, "MODEL_AUTH"],
  ["HTTP 429", { details: { status: 429 } }, "MODEL_RATE_LIMIT"],
  ["rate limit type", { error: { code: "rate_limit_exceeded", message: "secret" } }, "MODEL_RATE_LIMIT"],
  ["context with generic HTTP 400", { details: { status: 400, errorCode: "context_length_exceeded" } }, "MODEL_CONTEXT_OVERFLOW"],
  ["Pi nested context type", { details: { error: { type: "context_length_exceeded", message: "secret" } } }, "MODEL_CONTEXT_OVERFLOW"],
  ["network code", { error: { code: "ECONNRESET", message: "secret" } }, "MODEL_NETWORK"],
  ["transport failure", { type: "provider_transport_failure" }, "MODEL_NETWORK"],
  ["unknown HTTP 500", { details: { status: 500 } }, "MODEL_ERROR"],
  ["unstructured provider prose", {}, "MODEL_ERROR"],
];
for (const [name, diagnostic, code] of cases) {
  test(`structured model error: ${name}`, () => {
    const events: ProtocolEvent[] = [];
    const mapper = new EventMapper("session", "turn", (event) => events.push(event));
    const message = fauxAssistantMessage("", { stopReason: "error", errorMessage: "model unavailable; Authorization: secret" });
    message.diagnostics = [{ timestamp: Date.now(), type: "provider_failure", ...diagnostic }];
    mapper.accept({ type: "message_end", message });
    const error = events.find((event) => event.event === "error")!;
    assert.equal(error.code, code);
    assert.match(error.message, /[\u4e00-\u9fff]/);
    assert.equal(mapper.status, "error");
    assert.ok(!JSON.stringify(events).includes("secret"));
    // Classification is stable while the provider reason remains visible.
    message.errorMessage = "different provider text";
    mapper.accept({ type: "message_end", message });
    const second = events.filter((event) => event.event === "error")[1];
    assert.equal(second.code, code);
    assert.match(second.message, /different provider text/);
    assert.match(error.message, /model unavailable/);
  });
}


test("SDK OAuth exceptions become MODEL_AUTH without exposing causes", async () => {
  const { ModelsError } = await import("@earendil-works/pi-ai");
  const { modelException } = await import("../src/events/model-error.ts");
  const result = modelException(new ModelsError("oauth", "private-refresh-token"));
  assert.equal(result?.code, "MODEL_AUTH");
  assert.ok(!JSON.stringify(result).includes("private-refresh-token"));
  assert.equal(modelException(new Error("arbitrary provider prose")), undefined);
});

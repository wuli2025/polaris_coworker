// Test-only Node preload. Production main has no faux-provider switch.
import { registerFauxProvider } from "@earendil-works/pi-ai/compat";
import { fauxAssistantMessage, fauxToolCall } from "@earendil-works/pi-ai";

const faux = registerFauxProvider({ provider: "e2e-faux", api: "e2e-faux-api",
  models: [{ id: "e2e-model" }], tokensPerSecond: Infinity });
faux.setResponses([
  fauxAssistantMessage(fauxToolCall("read", { path: "sample.txt" }), { stopReason: "toolUse" }),
  (context) => {
    const text = JSON.stringify(context);
    return fauxAssistantMessage(text.includes("tool-result-marker") ? "tool round trip complete" : "tool result missing");
  },
  (_context, options) => new Promise((resolve) => {
    const timer = setTimeout(() => resolve(fauxAssistantMessage("late")), 10000);
    options?.signal?.addEventListener("abort", () => {
      clearTimeout(timer);
      resolve(fauxAssistantMessage("", { stopReason: "aborted" }));
    }, { once: true });
  }),
]);

import { Type } from "typebox";
import type { ExtensionFactory } from "@earendil-works/pi-coding-agent";
export function subagentFactory(run: (callId: string, task: string, signal?: AbortSignal) => Promise<string>): ExtensionFactory {
  return (pi) => pi.registerTool({ name: "subagent", label: "Subagent", description: "Delegate a bounded task to a child session with inherited permissions and sandbox",
    parameters: Type.Object({ task: Type.String({ minLength: 1 }) }),
    async execute(callId, args, signal) {
      return { content: [{ type: "text", text: await run(callId, args.task, signal) }], details: {} };
    } });
}

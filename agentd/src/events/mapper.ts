import type { AgentSessionEvent } from "@earendil-works/pi-coding-agent";
import type { ProtocolEvent, TurnStatus } from "../protocol/events.ts";

type Payload = ProtocolEvent extends infer E
  ? E extends ProtocolEvent ? Omit<E, "type" | "sessionId" | "turnId"> : never : never;

/** One mapper per host prompt, spanning all SDK tool turns and queued inputs. */
export class EventMapper {
  status: TurnStatus = "completed";
  private textStreamed = false;
  private thinkingStreamed = false;
  constructor(
    readonly sessionId: string,
    readonly turnId: string,
    private readonly send: (event: ProtocolEvent) => void,
  ) {}
  emit(payload: Payload): void {
    this.send({ type: "event", sessionId: this.sessionId, turnId: this.turnId, ...payload });
  }
  error(code: string, message: string): void {
    this.emit({ event: "error", code, message });
  }
  accept(event: AgentSessionEvent): void {
    if (event.type === "message_start" && event.message.role === "assistant") {
      this.textStreamed = false;
      this.thinkingStreamed = false;
    } else if (event.type === "message_update") {
      const update = event.assistantMessageEvent;
      if (update.type === "text_delta") {
        this.textStreamed = true;
        this.emit({ event: "text.delta", text: update.delta });
      } else if (update.type === "thinking_delta") {
        this.thinkingStreamed = true;
        this.emit({ event: "reasoning.delta", text: update.delta });
      }
    } else if (event.type === "tool_execution_start") {
      this.emit({ event: "tool.started", callId: event.toolCallId, tool: event.toolName,
        args: event.args, ...(event.parentToolCallId ? { parentCallId: event.parentToolCallId } : {}) });
    } else if (event.type === "tool_execution_end") {
      this.emit({ event: "tool.completed", callId: event.toolCallId, tool: event.toolName,
        result: event.result, isError: event.isError,
        ...(event.parentToolCallId ? { parentCallId: event.parentToolCallId } : {}) });
    } else if (event.type === "message_end" && event.message.role === "assistant") {
      const message = event.message;
      for (const block of message.content) {
        if (block.type === "text" && !this.textStreamed) this.emit({ event: "text.delta", text: block.text });
        if (block.type === "thinking" && !this.thinkingStreamed) {
          this.emit({ event: "reasoning.delta", text: block.thinking });
        }
      }
      this.emit({ event: "usage", usage: message.usage });
      // Provider error strings can include headers. Never forward them.
      if (message.stopReason === "error") {
        this.status = "error";
        this.error("MODEL_ERROR", "Model request failed");
      } else if (message.stopReason === "aborted") this.status = "aborted";
      else this.status = "completed"; // A successful automatic retry supersedes its failed attempt.
    }
  }
}

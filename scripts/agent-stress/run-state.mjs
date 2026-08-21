function parseInputTokens(text) {
  if (!text) return null;
  try {
    const value = JSON.parse(text);
    if (Number.isFinite(value)) return value;
    return Number.isFinite(value.inputTokens) ? value.inputTokens : null;
  } catch {
    return null;
  }
}

export class RunState {
  constructor({ requestId, conversationId, timeoutMs, now = Date.now }) {
    this.requestId = requestId;
    this.conversationId = conversationId;
    this.timeoutMs = timeoutMs;
    this.now = now;
    this.startedAt = now();
    this.firstEventAt = null;
    this.firstDeltaAt = null;
    this.finishedAt = null;
    this.terminalCount = 0;
    this.inputTokens = null;
    this.text = "";
    this.tools = [];
    this.artifacts = [];
    this.errors = [];
    this.timedOut = false;
  }

  accept(frame) {
    if (frame?.topic !== "chat:stream") return false;
    const event = frame.payload;
    if (!event || event.reqId !== this.requestId || event.conversationId !== this.conversationId) {
      return false;
    }

    const at = this.now();
    if (this.firstEventAt == null) this.firstEventAt = at;
    switch (event.kind) {
      case "meta": {
        const tokens = parseInputTokens(event.text);
        if (tokens != null) this.inputTokens = tokens;
        break;
      }
      case "delta":
        if (this.firstDeltaAt == null) this.firstDeltaAt = at;
        this.text += event.text || "";
        break;
      case "tool":
        this.tools.push({ name: event.tool || "", detail: event.text || "" });
        break;
      case "artifact":
        if (event.text) this.artifacts.push(event.text);
        break;
      case "error":
        if (event.text) this.errors.push(event.text);
        break;
      case "done":
        this.terminalCount += 1;
        this.finishedAt = at;
        break;
      default:
        break;
    }
    return true;
  }

  checkTimeout() {
    if (this.finishedAt != null) return false;
    const at = this.now();
    if (at - this.startedAt < this.timeoutMs) return false;
    this.timedOut = true;
    this.finishedAt = at;
    return true;
  }

  result() {
    return {
      requestId: this.requestId,
      conversationId: this.conversationId,
      terminalCount: this.terminalCount,
      startedAt: this.startedAt,
      firstEventAt: this.firstEventAt,
      firstDeltaAt: this.firstDeltaAt,
      finishedAt: this.finishedAt,
      inputTokens: this.inputTokens,
      text: this.text,
      tools: [...this.tools],
      artifacts: [...this.artifacts],
      errors: [...this.errors],
      timedOut: this.timedOut,
    };
  }
}

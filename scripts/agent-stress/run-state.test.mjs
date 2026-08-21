import { describe, expect, it } from "vitest";

import { RunState } from "./run-state.mjs";

function clock(...values) {
  let index = 0;
  return () => values[Math.min(index++, values.length - 1)];
}

function frame(reqId, conversationId, kind, fields = {}) {
  return {
    topic: "chat:stream",
    payload: { reqId, conversationId, kind, ...fields },
  };
}

describe("RunState", () => {
  it("isolates events by request and conversation while collecting observable metrics", () => {
    const state = new RunState({
      requestId: "r1",
      conversationId: "c1",
      timeoutMs: 1000,
      now: clock(100, 120, 140, 170, 210, 240),
    });

    state.accept(frame("r2", "c2", "delta", { text: "wrong" }));
    state.accept(frame("r1", "c1", "meta", { text: "{\"inputTokens\":321}" }));
    state.accept(frame("r1", "c1", "tool", { tool: "Read", text: "fixture.md" }));
    state.accept(frame("r1", "c1", "delta", { text: "hello" }));
    state.accept(frame("r1", "c1", "artifact", { text: "/tmp/result.pptx" }));
    state.accept(frame("r1", "c1", "done"));

    expect(state.result()).toEqual({
      requestId: "r1",
      conversationId: "c1",
      terminalCount: 1,
      startedAt: 100,
      firstEventAt: 120,
      firstDeltaAt: 170,
      finishedAt: 240,
      inputTokens: 321,
      text: "hello",
      tools: [{ name: "Read", detail: "fixture.md" }],
      artifacts: ["/tmp/result.pptx"],
      errors: [],
      timedOut: false,
    });
  });

  it("keeps duplicate terminal events visible so validation cannot report a false pass", () => {
    const state = new RunState({
      requestId: "r1",
      conversationId: "c1",
      timeoutMs: 1000,
      now: clock(10, 20, 30, 40),
    });

    state.accept(frame("r1", "c1", "done"));
    state.accept(frame("r1", "c1", "done"));

    expect(state.result().terminalCount).toBe(2);
  });

  it("marks a run timed out without inventing a done event", () => {
    const state = new RunState({
      requestId: "r1",
      conversationId: "c1",
      timeoutMs: 50,
      now: clock(100, 151, 151),
    });

    expect(state.checkTimeout()).toBe(true);
    expect(state.result()).toMatchObject({ terminalCount: 0, timedOut: true, finishedAt: 151 });
  });
});

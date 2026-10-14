import { expect, it, vi } from "vitest";
import { createDeltaBuffer } from "./chatDeltas";
import type { Bubble } from "./chatTypes";
it("keeps deltas ordered and never appends response text to error bubbles", () => {
  vi.useFakeTimers();
  const rows: Record<string, Bubble[]> = { a: [{ role: "assistant", text: "failed", err: true }], b: [] };
  const buffer = createDeltaBuffer(id => rows[id], 10);
  buffer.enqueue("a", "one");
  buffer.enqueue("a", "two");
  buffer.enqueue("b", "separate");
  expect(rows.a).toHaveLength(1);
  buffer.flush("a");
  rows.a.push({ role: "tool", text: "tool" });
  buffer.enqueue("a", "after tool");
  vi.advanceTimersByTime(10);
  expect(rows.a.map(row => row.text)).toEqual(["failed", "onetwo", "tool", "after tool"]);
  expect(rows.b[0].text).toBe("separate");
  vi.useRealTimers();
});

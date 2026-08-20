import { describe, expect, it } from "vitest";
import { buildTurnsSlice } from "./shared";
import type { Bubble } from "../../stores/chat";

function turnOf(bubbles: Bubble[]) {
  const turns = buildTurnsSlice(
    [{ role: "user", text: "do it" }, ...bubbles],
    0,
  );
  expect(turns).toHaveLength(1);
  return turns[0];
}

describe("chat turn timeline", () => {
  it("keeps text-tool-text arrival order", () => {
    const turn = turnOf([
      { role: "assistant", text: "先检查。" },
      { role: "tool", text: "", tool: "Read", toolDetail: "/repo/a.ts" },
      { role: "assistant", text: "检查完成。" },
    ]);

    expect(turn.timeline.map((item) => item.kind)).toEqual(["text", "tool", "text"]);
    expect(turn.text).toBe("先检查。\n\n检查完成。");
  });

  it("folds only truly consecutive calls of the same tool", () => {
    const turn = turnOf([
      { role: "tool", text: "", tool: "Read", toolDetail: "/repo/a.ts" },
      { role: "tool", text: "", tool: "Read", toolDetail: "/repo/b.ts" },
      { role: "assistant", text: "中间结果" },
      { role: "tool", text: "", tool: "Read", toolDetail: "/repo/c.ts" },
    ]);

    expect(turn.timeline.map((item) => item.kind)).toEqual(["tool", "text", "tool"]);
    expect(turn.tools).toHaveLength(2);
    expect(turn.tools[0]).toMatchObject({ count: 2, details: ["/repo/a.ts", "/repo/b.ts"] });
    expect(turn.tools[1]).toMatchObject({ count: 1, details: ["/repo/c.ts"] });
  });

  it("renders stderr warnings in place and continues normal text", () => {
    const turn = turnOf([
      {
        role: "assistant",
        text: "[警告] optional cache unavailable",
        severity: "warning",
      },
      { role: "assistant", text: "任务仍已完成。" },
    ]);

    expect(turn.timeline).toMatchObject([
      { kind: "text", severity: "warning" },
      { kind: "text", severity: "normal" },
    ]);
    expect(turn.hasAssistant).toBe(true);
    expect(turn.text).toBe("任务仍已完成。");
  });

  it("keeps fatal, send-failure and timeout bubbles as errors", () => {
    for (const text of [
      "[错误] process exited 1",
      "[发送失败] backend unavailable",
      "[本轮超时] backend stopped",
    ]) {
      const turn = turnOf([{ role: "assistant", text, err: true, severity: "error" }]);
      expect(turn.timeline).toMatchObject([{ kind: "text", severity: "error" }]);
      expect(turn.hasAssistant).toBe(false);
      expect(turn.errors).toEqual([text]);
    }
  });

  it("classifies old bubbles without severity fields", () => {
    const turn = turnOf([
      { role: "assistant", text: "[stderr] old backend diagnostic" },
      { role: "assistant", text: "[result error] old backend failure" },
    ]);
    expect(turn.timeline).toMatchObject([
      { kind: "text", severity: "warning" },
      { kind: "text", severity: "error" },
    ]);
  });
});

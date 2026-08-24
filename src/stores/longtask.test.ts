import { createPinia, setActivePinia } from "pinia";
import { beforeEach, describe, expect, it, vi } from "vitest";

const harness = vi.hoisted(() => {
  const manifests: Array<{ units: Array<{ id: string; title: string; status: string }> }> = [];
  const sends: unknown[][] = [];
  const bubbles: Array<{ convId: string; bubble: { role: string; text: string } }> = [];
  const events: string[] = [];
  const outcomes: Array<"success" | "error" | "cancelled"> = [];
  return {
    manifests,
    sends,
    bubbles,
    events,
    outcomes,
    chat: {
      inputTokens: () => 1_000,
      send: async (...args: unknown[]) => {
        sends.push(args);
        events.push("send");
      },
      waitForDone: async () => {
        events.push("done");
        return outcomes.shift() ?? "success";
      },
      pushBubble: (convId: string, bubble: { role: string; text: string }) => {
        bubbles.push({ convId, bubble });
        events.push("bubble");
      },
    },
  };
});

vi.mock("../tauri", () => ({
  chat: {
    buildManifest: async () => harness.manifests.shift() ?? null,
  },
}));

vi.mock("./chat", () => ({
  useChatStore: () => harness.chat,
}));

import { useLongTaskStore } from "./longtask";

describe("long task final delivery audit", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    harness.manifests.length = 0;
    harness.sends.length = 0;
    harness.bubbles.length = 0;
    harness.events.length = 0;
    harness.outcomes.length = 0;
  });

  it("runs one audit turn after every manifest unit is done before reporting completion", async () => {
    harness.manifests.push({
      units: [{ id: "slide-1", title: "封面", status: "done" }],
    });

    const store = useLongTaskStore();
    await store.runBatchBuild("conv-1", "生成一页演示并同时交付 HTML", "生成演示", {
      permissionMode: "auto_all",
      skillIds: ["polaris-deck-studio"],
      workMode: "work",
    });

    expect(harness.sends).toHaveLength(2);
    expect(harness.sends[0][1]).toBe("生成一页演示并同时交付 HTML");
    expect(harness.sends[1][1]).toEqual(expect.stringContaining("最终交付审计"));
    expect(harness.sends[1][1]).toEqual(expect.stringContaining("原始用户要求"));
    expect(harness.sends[1][1]).toEqual(expect.stringContaining("缺失"));
    expect(harness.sends[0][4]).toEqual(expect.objectContaining({ workMode: "work" }));
    expect(harness.sends[1][4]).toEqual(expect.objectContaining({ workMode: "work" }));
    expect(harness.events.at(-1)).toBe("bubble");
    expect(harness.bubbles.at(-1)?.bubble.text).toContain("分批长任务完成");
  });

  it("does not report completion when the final audit turn fails", async () => {
    harness.manifests.push({
      units: [{ id: "slide-1", title: "封面", status: "done" }],
    });
    harness.outcomes.push("success", "error");

    const store = useLongTaskStore();
    await store.runBatchBuild("conv-2", "生成一页演示", "生成演示", {
      permissionMode: "auto_all",
      skillIds: ["polaris-deck-studio"],
      workMode: "fast",
    });

    expect(harness.sends).toHaveLength(2);
    expect(harness.bubbles.at(-1)?.bubble.text).toContain("最终交付审计失败");
    expect(harness.bubbles.some(({ bubble }) => bubble.text.includes("分批长任务完成"))).toBe(false);
  });
});

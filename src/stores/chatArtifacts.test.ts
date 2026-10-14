import { beforeEach, expect, it, vi } from "vitest";
const state = vi.hoisted(() => ({
  app: { currentConvId: "current", drawerCollapsed: true },
  artifacts: { current: null as { path: string } | null, open: vi.fn() },
}));
vi.mock("./app", () => ({ useAppStore: () => state.app }));
vi.mock("./artifacts", () => ({ useArtifactsStore: () => state.artifacts }));
import { parseArtifacts, previewDeliverable, receiveArtifact } from "./chatArtifacts";
import type { Bubble } from "./chatTypes";
beforeEach(() => {
  state.app.drawerCollapsed = true;
  state.artifacts.current = null;
  state.artifacts.open.mockClear();
});
it("keeps user-visible artifacts while filtering implementation files", () => {
  expect(parseArtifacts('answer<!--POLARIS_ARTIFACTS:["page.html","code.ts","polaris.slides.json"]-->'))
    .toEqual({ text: "answer", artifacts: ["page.html", "polaris.slides.json"] });
});
it("attaches artifacts to a response and preserves user focus in the current conversation", () => {
  const rows: Bubble[] = [{ role: "assistant", text: "error", err: true }];
  receiveArtifact(rows, "current", "/conversations/current/polaris.slides.json");
  expect(rows).toHaveLength(2);
  expect(rows[0].artifacts).toBeUndefined();
  expect(state.artifacts.open).toHaveBeenCalledTimes(1);
  receiveArtifact(rows, "current", "/conversations/current/polaris.slides.json");
  expect(state.artifacts.open).toHaveBeenCalledTimes(1);
  state.artifacts.open.mockClear();
  state.artifacts.current = { path: "/conversations/current/user-choice.pdf" };
  previewDeliverable(rows, "current");
  expect(state.artifacts.open).not.toHaveBeenCalled();
  state.artifacts.current = null;
  previewDeliverable(rows, "background");
  expect(state.artifacts.open).not.toHaveBeenCalled();
});

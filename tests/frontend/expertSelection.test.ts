import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createPinia, setActivePinia } from "pinia";

const ports = vi.hoisted(() => ({
  invoke: vi.fn(), apply: vi.fn(), teamApply: vi.fn(), send: vi.fn(),
}));
vi.mock("../../src/tauri", () => ({
  invoke: ports.invoke,
  expert: { apply: ports.apply, teamApply: ports.teamApply },
  chat: { send: ports.send }, convApi: {}, listen: vi.fn().mockResolvedValue(() => {}),
}));
vi.mock("../../src/stores/app", () => ({ useAppStore: () => ({ markUnread: vi.fn() }) }));
vi.mock("../../src/stores/artifacts", () => ({ useArtifactsStore: () => ({}) }));
vi.mock("../../src/features/coworker/stores/sessions", () => ({
  useSessionsStore: () => ({ start: vi.fn(), finish: vi.fn() }),
}));
import { applyExpertSelection, selectedExpert } from "../../src/lib/expertSelection";
import { useChatStore } from "../../src/stores/chat";

let project: string;
beforeEach(() => {
  vi.clearAllMocks();
  vi.stubGlobal("window", { setInterval, clearInterval });
  setActivePinia(createPinia());
  project = mkdtempSync(join(tmpdir(), "polaris-expert-"));
  for (const name of ["CLAUDE.md", "AGENTS.md"]) writeFileSync(join(project, name), `user ${name}`);
  ports.apply.mockImplementation(async () => writeFileSync(join(project, "CLAUDE.md"), "legacy expert"));
  ports.teamApply.mockImplementation(async () => writeFileSync(join(project, "CLAUDE.md"), "legacy team"));
  ports.send.mockResolvedValue("request");
});
afterEach(() => {
  rmSync(project, { recursive: true, force: true });
  vi.unstubAllGlobals();
});

describe("request-scoped expert selection", () => {
  it.each(["expert", "team"] as const)("Pi %s selection preserves project files and reaches chat_send", async kind => {
    ports.invoke.mockResolvedValue({ backend: "pi" });
    await applyExpertSelection("project", kind, "chosen-id");
    const selection = selectedExpert(kind === "team" ? "expert-team" : "single-expert", "chosen-id", "chosen-id");
    await useChatStore().send("conv", "task", "task", undefined, {
      permissionMode: "deny", skillIds: [], expertSelection: selection,
    });
    expect(ports.apply).not.toHaveBeenCalled();
    expect(ports.teamApply).not.toHaveBeenCalled();
    expect(ports.send).toHaveBeenCalledWith(expect.objectContaining({ expertSelection: { kind, id: "chosen-id" } }));
    for (const name of ["CLAUDE.md", "AGENTS.md"]) expect(readFileSync(join(project, name), "utf8")).toBe(`user ${name}`);
  });
  it.each(["expert", "team"] as const)("legacy %s keeps apply behavior", async kind => {
    ports.invoke.mockResolvedValue({ backend: "claude-cli" });
    await applyExpertSelection("project", kind, "chosen-id");
    expect(kind === "team" ? ports.teamApply : ports.apply).toHaveBeenCalledWith("project", "chosen-id", true);
    expect(readFileSync(join(project, "CLAUDE.md"), "utf8")).toBe(`legacy ${kind}`);
  });
  it("fails closed if backend detection fails and clears selection in basic modes", async () => {
    ports.invoke.mockRejectedValue(new Error("offline"));
    await expect(applyExpertSelection("p", "expert", "id")).rejects.toThrow("offline");
    expect(ports.apply).not.toHaveBeenCalled();
    for (const mode of ["single-agent", "auto-match"]) expect(selectedExpert(mode, "old", "old")).toBeUndefined();
  });
});

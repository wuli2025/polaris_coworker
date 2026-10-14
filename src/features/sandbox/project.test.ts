import { beforeEach, expect, it, vi } from "vitest";
const invoke = vi.hoisted(() => vi.fn());
vi.mock("../../tauri", () => ({ invoke }));
import { needsSandboxMigration, projectDirectoryHint, startProjectSandbox } from "./project";
beforeEach(() => { invoke.mockReset(); });
it("starts only a confirmed project and passes the explicit rebuild flag", async () => {
  invoke.mockResolvedValue("preserved old container");
  await startProjectSandbox(" D:/My Project ");
  expect(invoke).toHaveBeenLastCalledWith("sandbox_start", { projectDir: "D:/My Project", rebuild: false });
  await startProjectSandbox("D:/My Project", true);
  expect(invoke).toHaveBeenLastCalledWith("sandbox_start", { projectDir: "D:/My Project", rebuild: true });
  invoke.mockClear();
  await expect(startProjectSandbox(" ")).rejects.toThrow("请选择");
  expect(invoke).not.toHaveBeenCalled();
});
it("shows migration for unsafe mounts, security baselines and changed workdirs", () => {
  for (const code of ["UNSAFE_MOUNT", "UNSAFE_CONTAINER", "PATH_UNMOUNTED", "INVALID_MOUNTS"]) {
    expect(needsSandboxMigration([`SANDBOX_${code}: rebuild required`])).toBe(true);
  }
  expect(needsSandboxMigration(["Docker unavailable"])).toBe(false);
});

it("gives immediate POSIX and Windows drive format guidance", () => {
  for (const value of ["/home/me/project", "/", "C:/Projects/app", "d:\\Projects\\app", " D:/My Project "]) {
    expect(projectDirectoryHint(value)).toBe("");
  }
  for (const value of ["app", "./app", "../app", "~/app", "C:app", "C:", "\\app"]) {
    expect(projectDirectoryHint(value)).toContain("绝对路径");
  }
  expect(projectDirectoryHint(" ")).toContain("请选择");
});
it("keeps the backend authoritative for paths that pass the format hint", async () => {
  invoke.mockRejectedValue(new Error("SANDBOX_UNSAFE_MOUNT: protected host data"));
  expect(projectDirectoryHint("/home/me/.ssh")).toBe("");
  await expect(startProjectSandbox("/home/me/.ssh")).rejects.toThrow("SANDBOX_UNSAFE_MOUNT");
});

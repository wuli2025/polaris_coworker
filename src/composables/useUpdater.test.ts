import { describe, expect, it } from "vitest";
import {
  applyDesktopUpdaterState,
  currentVersion,
  dockerStatusMessage,
  isDockerStatus,
  updateError,
  updateNotes,
  updateProgress,
  updateVersion,
  updating,
  upToDate,
} from "./useUpdater";

describe("desktop updater protocol", () => {
  it("maps every backend state used by the update dialog", () => {
    applyDesktopUpdaterState({
      current_version: "2.9.2",
      status: "available",
      version: "2.10.0",
      notes: "remote update",
    });
    expect(currentVersion.value).toBe("2.9.2");
    expect(updateVersion.value).toBe("2.10.0");
    expect(updateNotes.value).toBe("remote update");
    expect(upToDate.value).toBe(false);

    applyDesktopUpdaterState({
      current_version: "2.9.2",
      status: "downloading",
      version: "2.10.0",
      percent: 41,
    });
    expect(updating.value).toBe(true);
    expect(updateProgress.value).toBe(41);

    applyDesktopUpdaterState({ current_version: "2.10.0", status: "up-to-date" });
    expect(updateVersion.value).toBeNull();
    expect(updating.value).toBe(false);
    expect(upToDate.value).toBe(true);

    applyDesktopUpdaterState({
      current_version: "2.10.0",
      status: "error",
      message: "signature rejected",
    });
    expect(updateError.value).toBe("signature rejected");
    expect(updating.value).toBe(false);
  });
});

describe("Docker updater setup state", () => {
  it("accepts old and new server status payloads without requiring access-auth telemetry", () => {
    expect(
      isDockerStatus({
        updater_enabled: true,
        updater_service: true,
        update_script: true,
      }),
    ).toBe(true);
  });

  it("guides legacy images to the one-time migration without asking for a password", () => {
    const message = dockerStatusMessage({
      updater_enabled: false,
      updater_service: false,
      update_script: false,
    });

    expect(message).toBe("当前镜像没有更新脚本，请先执行官网的一次迁移命令");
    expect(message).not.toMatch(/口令|登录|token/i);
  });

  it("reports the invisible updater service separately from the user access model", () => {
    const message = dockerStatusMessage({
      updater_enabled: false,
      updater_service: false,
      update_script: true,
      auth_configured: false,
    });

    expect(message).toBe("内部更新服务尚未启动，请重新运行官网安装/迁移命令");
    expect(message).not.toMatch(/口令|登录|token/i);
  });
});

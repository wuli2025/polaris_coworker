import { describe, expect, it } from "vitest";
import { dockerStatusMessage, isDockerStatus } from "./useUpdater";

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

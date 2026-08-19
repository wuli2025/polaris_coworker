// 自动更新统一视图：桌面走 Tauri updater 状态机，Docker/Web 走同源 HTTP 命令。
// 两条运行时只在这里分叉，UpdateBanner / UpdatePanel 继续消费同一组派生状态。
import { computed, ref } from "vue";
import { getVersion } from "@tauri-apps/api/app";
import { invoke, isTauri, listen } from "../tauri";

type UpdaterState =
  | { status: "disabled" }
  | { status: "idle" }
  | { status: "checking" }
  | { status: "up-to-date" }
  | { status: "available"; version: string; notes: string }
  | { status: "downloading"; version: string; percent: number }
  | { status: "ready"; version: string }
  | { status: "installing"; version: string }
  | { status: "error"; message: string };

type DockerStatus = {
  updater_enabled: boolean;
  updater_service: boolean;
  update_script: boolean;
  auth_configured: boolean;
  current_version: string;
  current_tag: string;
  image_repo: string;
};

type DockerCheck = {
  ok: boolean;
  current?: string;
  latest?: string;
  has_update: boolean;
  image?: string;
  source?: string;
  error?: string;
};

type DockerUpdate = {
  success: boolean;
  exit_code?: number | null;
  stdout?: string;
  stderr?: string;
  note?: string;
};

const state = ref<UpdaterState>({ status: "idle" });
export const updaterRuntime = ref<"desktop" | "docker" | "browser">(
  isTauri ? "desktop" : "browser"
);
export const dockerUpdaterEnabled = ref(false);
export const dockerUpdaterServiceConfigured = ref(false);
export const dockerUpdateScriptPresent = ref(false);
export const dockerAuthConfigured = ref(false);
export const dockerMessage = ref("");

export const currentVersion = ref<string>("");
export const lastCheckedAt = ref<number | null>(null);
export const dialogDismissed = ref(false);

const versionOf = (s: UpdaterState): string | null =>
  "version" in s ? s.version : null;

export const updateVersion = computed<string | null>(() => versionOf(state.value));
export const remoteVersion = updateVersion;
export const updateNotes = computed<string>(() =>
  state.value.status === "available" ? state.value.notes : ""
);
export const updating = computed(
  () => state.value.status === "downloading" || state.value.status === "installing"
);
export const updateProgress = computed(() => {
  const s = state.value;
  if (s.status === "downloading") return s.percent;
  if (s.status === "installing" || s.status === "ready") return 100;
  return 0;
});
export const updateError = computed(() =>
  state.value.status === "error" ? state.value.message : ""
);
export const checking = computed(() => state.value.status === "checking");
export const upToDate = computed(() => state.value.status === "up-to-date");
export const checkFailed = computed(() => state.value.status === "error");

let subscribed = false;
let autoChecked = false;
let dockerStatusLoaded = false;

async function loadDockerStatus(): Promise<boolean> {
  if (isTauri) return false;
  try {
    const status = await invoke<DockerStatus>("docker_status");
    updaterRuntime.value = "docker";
    dockerStatusLoaded = true;
    dockerUpdaterEnabled.value = !!status.updater_enabled;
    dockerUpdaterServiceConfigured.value = !!status.updater_service;
    dockerUpdateScriptPresent.value = !!status.update_script;
    dockerAuthConfigured.value = !!status.auth_configured;
    if (!currentVersion.value) currentVersion.value = status.current_version || status.current_tag || "—";
    return true;
  } catch {
    // npm run dev 的纯浏览器预览没有后端；它不是“更新失败”，只是没有更新能力。
    updaterRuntime.value = "browser";
    dockerStatusLoaded = false;
    return false;
  }
}

/** 供更新页挂载时只加载运行时/当前版本，不触发网络版本检查。 */
export async function loadUpdaterVersion(): Promise<void> {
  if (currentVersion.value) return;
  if (isTauri) {
    try {
      currentVersion.value = await getVersion();
    } catch {
      /* Tauri 初始化早期拿不到时，下一次检查再补。 */
    }
    return;
  }
  await loadDockerStatus();
}

async function ensureSubscribed(): Promise<void> {
  if (!isTauri || subscribed) return;
  subscribed = true;
  try {
    await listen<UpdaterState>("updater://state", (payload) => {
      state.value = payload;
    });
    state.value = await invoke<UpdaterState>("updater_get_state");
  } catch (e) {
    subscribed = false;
    console.warn("[updater] subscribe failed:", e);
  }
}

async function checkDocker(): Promise<UpdaterState> {
  if (!dockerStatusLoaded && !(await loadDockerStatus())) {
    return { status: "disabled" };
  }
  state.value = { status: "checking" };
  const result = await invoke<DockerCheck>("docker_check_update");
  lastCheckedAt.value = Date.now();
  if (result.current) currentVersion.value = result.current;
  if (!result.ok) {
    return {
      status: "error",
      message: result.error || "Docker 版本源暂时不可用",
    };
  }
  if (result.has_update && result.latest) {
    return {
      status: "available",
      version: result.latest,
      notes: `Docker 镜像 ${result.image || ""} 已有新版本。更新会在后台替换容器，数据卷与配置保持不变。`,
    };
  }
  return { status: "up-to-date" };
}

async function checkOnce(): Promise<UpdaterState> {
  await loadUpdaterVersion();
  if (isTauri) {
    await ensureSubscribed();
    const result = await invoke<UpdaterState>("updater_check");
    lastCheckedAt.value = Date.now();
    return result;
  }
  return checkDocker();
}

/** 启动时错峰检查；仅网络错误退避，拿到确定结论立即收手。 */
export async function checkForUpdate(): Promise<void> {
  if (autoChecked) return;
  autoChecked = true;
  const delays = [5000, 4000, 12000, 30000];
  for (const wait of delays) {
    await new Promise((resolve) => setTimeout(resolve, wait));
    try {
      const result = await checkOnce();
      state.value = result;
      if (result.status === "disabled" || result.status !== "error") return;
    } catch (e) {
      state.value = { status: "error", message: String(e) };
      console.warn("[updater] auto check failed, will retry:", e);
    }
  }
}

export async function manualCheck(): Promise<void> {
  dialogDismissed.value = false;
  try {
    state.value = { status: "checking" };
    state.value = await checkOnce();
  } catch (e) {
    state.value = { status: "error", message: String(e) };
    console.warn("[updater] manual check failed:", e);
  }
}

/**
 * 桌面：下载安装并由 Tauri 重启。
 * Docker：让 server 调用隔离 Watchtower sidecar 的窄 HTTP API；当前容器随后会断线并被重建。
 */
export async function applyUpdate(force = false): Promise<void> {
  if (updating.value) return;
  if (isTauri) {
    try {
      await invoke("updater_apply");
    } catch (e) {
      state.value = { status: "error", message: String(e) };
      console.warn("[updater] apply failed:", e);
    }
    return;
  }

  if (updaterRuntime.value !== "docker") return;
  const version = updateVersion.value || currentVersion.value || "latest";
  state.value = { status: "installing", version };
  dockerMessage.value = "正在通知隔离更新服务…";
  try {
    const result = await invoke<DockerUpdate>("docker_update", {
      confirm: true,
      force,
    });
    if (!result.success) {
      throw new Error(result.stderr || result.stdout || `更新脚本退出码 ${result.exit_code ?? "未知"}`);
    }
    dockerMessage.value =
      result.note || "隔离更新服务已接单。容器会在拉取完成后短暂断线，约 1–3 分钟后刷新即可。";
    // 保持 installing：正常路径会先断线再由新容器接棒，不能在旧页面假装已经完成。
  } catch (e) {
    dockerMessage.value = "";
    state.value = { status: "error", message: String(e) };
    console.warn("[updater] docker apply failed:", e);
  }
}

export function dismissUpdate(): void {
  dialogDismissed.value = true;
}

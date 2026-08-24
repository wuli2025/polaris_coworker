import { ref } from "vue";
import { invoke, listen, isTauri, BackendHttpError } from "../tauri";

export type UpdaterRuntime = "unknown" | "desktop" | "docker" | "browser";

export const DESKTOP_UPDATER_EVENT = "updater://state";

export type DesktopUpdaterSnapshot =
  | { current_version: string; status: "disabled" | "idle" | "checking" | "up-to-date" }
  | { current_version: string; status: "available"; version: string; notes: string }
  | { current_version: string; status: "downloading"; version: string; percent: number }
  | { current_version: string; status: "ready" | "installing"; version: string }
  | { current_version: string; status: "error"; message: string };

export const currentVersion = ref("");
export const updateVersion = ref<string | null>(null);
export const updateNotes = ref("");
export const updating = ref(false);
export const updateProgress = ref(0);
export const updateError = ref("");
export const checking = ref(false);
export const upToDate = ref(false);
export const checkFailed = ref(false);
export const dialogDismissed = ref(false);
export const lastCheckedAt = ref<number | null>(null);
export const updaterRuntime = ref<UpdaterRuntime>("unknown");

export const dockerUpdaterEnabled = ref(false);
export const dockerUpdaterServiceConfigured = ref(false);
export const dockerUpdateScriptPresent = ref(false);
export const dockerMessage = ref("");
export const dockerRequestId = ref<string | null>(null);

let versionLoaded = false;
let listenerReady = false;
let autoChecked = false;
let dockerPollGeneration = 0;
let lastDockerCheck: DockerCheck | null = null;

const DOCKER_UPDATE_TIMEOUT_MS = 15 * 60 * 1000;
const DOCKER_POLL_INTERVAL_MS = 2_000;

export function applyDesktopUpdaterState(snapshot: DesktopUpdaterSnapshot): void {
  if (snapshot.current_version) currentVersion.value = snapshot.current_version;

  if (snapshot.status === "available") {
    updateVersion.value = snapshot.version;
    updateNotes.value = snapshot.notes;
    updateProgress.value = 0;
    updateError.value = "";
    updating.value = false;
    upToDate.value = false;
  } else if (snapshot.status === "downloading") {
    updateVersion.value = snapshot.version;
    updateProgress.value = snapshot.percent;
    updateError.value = "";
    updating.value = true;
    upToDate.value = false;
  } else if (snapshot.status === "installing" || snapshot.status === "ready") {
    updateVersion.value = snapshot.version;
    updateProgress.value = snapshot.status === "ready" ? 100 : updateProgress.value;
    updateError.value = "";
    updating.value = true;
    upToDate.value = false;
  } else if (snapshot.status === "up-to-date") {
    updateVersion.value = null;
    updateNotes.value = "";
    updateProgress.value = 0;
    updateError.value = "";
    updating.value = false;
    upToDate.value = true;
  } else if (snapshot.status === "error") {
    updateError.value = snapshot.message;
    updating.value = false;
    upToDate.value = false;
  } else {
    updating.value = false;
  }
}

export interface DockerStatus {
  updater_enabled: boolean;
  updater_service: boolean;
  update_script: boolean;
  auth_configured?: boolean;
  current_version?: string;
  current_revision?: string;
  boot_id?: string;
  current_tag?: string;
  image_repo?: string;
}

interface DockerCheck {
  ok: boolean;
  current?: string;
  latest?: string;
  current_revision?: string;
  target_revision?: string;
  target_digest?: string;
  has_update: boolean;
  image?: string;
  source?: string;
  error?: string;
}

interface DockerUpdateAccepted {
  accepted: boolean;
  upToDate?: boolean;
  requestId?: string;
  targetRevision?: string;
  targetVersion?: string;
  targetDigest?: string;
  sourceBootId?: string;
  deadline?: number;
  note?: string;
}

export interface DockerBuild {
  bootId?: string;
  version?: string;
  buildRevision?: string;
}

export function replacementMatches(
  build: DockerBuild | null,
  sourceBootId: string,
  targetRevision: string,
): boolean {
  return Boolean(
    build?.bootId &&
      build.bootId !== sourceBootId &&
      build.buildRevision === targetRevision,
  );
}

interface DockerTriggerResult {
  state?: string;
  exitCode?: number | null;
  message?: string;
}

interface DockerUpdateStatus {
  requestId: string;
  state:
    | "queued"
    | "triggering"
    | "waiting_restart"
    | "succeeded"
    | "failed"
    | "unconfirmed";
  deadline?: number;
  sourceBootId?: string;
  sourceRevision?: string;
  targetRevision?: string;
  targetVersion?: string;
  targetDigest?: string;
  currentBuild?: DockerBuild;
  triggerResult?: DockerTriggerResult | null;
}

interface DockerWaitTarget {
  requestId?: string;
  sourceBootId: string;
  targetRevision: string;
  targetVersion?: string;
  deadlineMs: number;
}

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}

export function isDockerStatus(value: unknown): value is DockerStatus {
  if (!value || typeof value !== "object") return false;
  const status = value as Partial<DockerStatus>;
  return (
    typeof status.updater_enabled === "boolean" &&
    typeof status.updater_service === "boolean" &&
    typeof status.update_script === "boolean" &&
    (status.auth_configured === undefined || typeof status.auth_configured === "boolean")
  );
}

export function dockerStatusMessage(status: DockerStatus): string {
  return status.updater_enabled
    ? "更新服务已就绪"
    : !status.update_script
      ? "当前镜像没有更新脚本，请先执行官网的一次迁移命令"
      : !status.updater_service
        ? "内部更新服务尚未启动，请重新运行官网安装/迁移命令"
        : "更新服务尚未就绪";
}

async function readDockerBuild(): Promise<DockerBuild | null> {
  try {
    const response = await fetch("/api/build", { cache: "no-store" });
    if (!response.ok) return null;
    const value = (await response.json()) as DockerBuild;
    return value && typeof value === "object" ? value : null;
  } catch {
    return null;
  }
}

async function dockerReady(): Promise<boolean> {
  try {
    const response = await fetch("/api/ready", { cache: "no-store" });
    return response.ok;
  } catch {
    return false;
  }
}

function dockerFailureMessage(status: DockerUpdateStatus): string {
  const detail = status.triggerResult?.message?.trim();
  if (detail) return detail;
  if (status.triggerResult?.exitCode != null) {
    return `隔离更新服务执行失败（退出码 ${status.triggerResult.exitCode}）`;
  }
  return "隔离更新服务未能拉取并替换容器";
}

/**
 * Wait for observable replacement evidence. A Watchtower HTTP 200 is deliberately
 * insufficient: success means both a new process boot and the requested OCI revision.
 */
async function waitForDockerReplacement(target: DockerWaitTarget): Promise<void> {
  const generation = ++dockerPollGeneration;
  const startedAt = Date.now();
  const deadlineMs = Math.min(
    target.deadlineMs || startedAt + DOCKER_UPDATE_TIMEOUT_MS,
    startedAt + DOCKER_UPDATE_TIMEOUT_MS,
  );

  while (generation === dockerPollGeneration) {
    const now = Date.now();
    if (now >= deadlineMs) {
      throw new Error(
        "15 分钟内未观察到目标容器启动。更新没有被确认，请查看 docker logs polaris-updater 后重试。",
      );
    }

    const elapsed = now - startedAt;
    updateProgress.value = Math.min(92, 8 + Math.round((elapsed / (deadlineMs - startedAt)) * 84));

    const build = await readDockerBuild();
    if (replacementMatches(build, target.sourceBootId, target.targetRevision)) {
      dockerMessage.value = "目标容器已启动，正在等待服务就绪…";
      if (await dockerReady()) {
        updateProgress.value = 100;
        currentVersion.value = build.version || target.targetVersion || currentVersion.value;
        updateVersion.value = null;
        upToDate.value = true;
        dockerMessage.value = "更新完成，正在载入新容器…";
        await sleep(500);
        window.location.reload();
        return;
      }
    } else if (!build) {
      dockerMessage.value = "旧容器正在退出，等待新容器恢复…";
    } else if (build.bootId !== target.sourceBootId) {
      dockerMessage.value = "容器已经重启，正在核对目标镜像版本…";
    }

    if (target.requestId) {
      try {
        const status = await invoke<DockerUpdateStatus>("docker_update_status", {
          requestId: target.requestId,
        });
        if (status.state === "failed") {
          throw new Error(`${dockerFailureMessage(status)}。请查看 docker logs polaris-updater。`);
        }
        if (status.state === "unconfirmed") {
          throw new Error(
            "Watchtower 已返回，但没有观察到目标 build 启动；本次更新未确认。请查看 docker logs polaris-updater 后重试。",
          );
        }
        if (status.state === "waiting_restart") {
          dockerMessage.value = "镜像扫描已返回，等待目标容器实际替换…";
        } else if (status.state === "succeeded") {
          dockerMessage.value = "目标 build 已确认，等待服务就绪…";
        } else if (build) {
          dockerMessage.value = "隔离更新服务已接单，正在拉取并替换容器…";
        }
      } catch (error) {
        const message = errorText(error);
        // During a real replacement /api/invoke is expected to disconnect. Only
        // explicit terminal states stop polling; transport failures remain recoverable.
        if (
          message.includes("隔离更新服务执行失败") ||
          message.includes("本次更新未确认") ||
          message.includes("docker logs polaris-updater")
        ) {
          throw error;
        }
      }
    }

    await sleep(DOCKER_POLL_INTERVAL_MS);
  }
}

async function loadDockerStatus(): Promise<boolean> {
  try {
    const raw = await invoke<unknown>("docker_status");
    if (!isDockerStatus(raw)) {
      updaterRuntime.value = "browser";
      return false;
    }
    const status = raw;
    updaterRuntime.value = "docker";
    dockerUpdaterEnabled.value = status.updater_enabled;
    dockerUpdaterServiceConfigured.value = status.updater_service;
    dockerUpdateScriptPresent.value = status.update_script;
    if (status.current_version) currentVersion.value = status.current_version;
    dockerMessage.value = dockerStatusMessage(status);
    return true;
  } catch (error) {
    // Reaching a server that rejects or fails docker_status is not browser preview.
    updaterRuntime.value = "docker";
    dockerUpdaterEnabled.value = false;
    dockerMessage.value =
      error instanceof BackendHttpError && error.status === 401
        ? "请先登录团队账号或输入正确的 POLARIS_AUTH_TOKEN，才能读取更新状态"
        : error instanceof BackendHttpError && error.status === 403
          ? "当前账号不是 owner，不能管理容器更新"
          : `Docker 更新服务异常：${errorText(error)}`;
    updateError.value = dockerMessage.value;
    return false;
  }
}

async function checkDocker(): Promise<void> {
  const isDocker = await loadDockerStatus();
  if (!isDocker) {
    if (updaterRuntime.value === "browser") {
      upToDate.value = true;
      checkFailed.value = false;
    } else {
      checkFailed.value = true;
    }
    return;
  }

  const result = await invoke<DockerCheck>("docker_check_update");
  lastDockerCheck = result;
  if (!result.ok) {
    throw new Error(result.error || "OCI 镜像版本检查失败");
  }

  if (result.current) currentVersion.value = result.current;
  if (result.has_update) {
    updateVersion.value = result.latest || result.target_revision?.slice(0, 12) || "新版镜像";
    const revision = result.target_revision?.slice(0, 12);
    updateNotes.value = revision
      ? `目标镜像 ${result.image || ""}\nBuild ${revision}`.trim()
      : `目标镜像 ${result.image || ""}`.trim();
    upToDate.value = false;
  } else {
    updateVersion.value = null;
    updateNotes.value = "";
    upToDate.value = true;
  }
  checkFailed.value = false;
}

export async function loadUpdaterVersion(): Promise<void> {
  if (versionLoaded) return;
  versionLoaded = true;

  if (!isTauri) {
    const docker = await loadDockerStatus();
    if (!docker && updaterRuntime.value === "browser") currentVersion.value ||= "Web";
    return;
  }

  updaterRuntime.value = "desktop";
  try {
    const state = await invoke<DesktopUpdaterSnapshot>("updater_get_state");
    applyDesktopUpdaterState(state);
  } catch {
    // updater state is optional on unsupported desktop builds
  }
}

async function ensureListener(): Promise<void> {
  if (listenerReady || !isTauri) return;
  listenerReady = true;
  await listen<DesktopUpdaterSnapshot>(DESKTOP_UPDATER_EVENT, (snapshot) => {
    if (snapshot) applyDesktopUpdaterState(snapshot);
  });
}

export async function autoCheck(): Promise<void> {
  if (autoChecked) return;
  autoChecked = true;
  await loadUpdaterVersion();
  if (isTauri) await ensureListener();

  checking.value = true;
  updateError.value = "";
  checkFailed.value = false;
  try {
    if (!isTauri) {
      await checkDocker();
    } else {
      const snapshot = await invoke<DesktopUpdaterSnapshot>("updater_check");
      applyDesktopUpdaterState(snapshot);
    }
  } catch (error) {
    checkFailed.value = true;
    updateError.value = errorText(error);
  } finally {
    checking.value = false;
    lastCheckedAt.value = Date.now();
  }
}

export async function checkForUpdate(): Promise<void> {
  await autoCheck();
}

export async function manualCheck(): Promise<void> {
  await loadUpdaterVersion();
  if (isTauri) await ensureListener();
  checking.value = true;
  updateError.value = "";
  checkFailed.value = false;
  upToDate.value = false;
  dialogDismissed.value = false;
  try {
    if (!isTauri) {
      await checkDocker();
    } else {
      const snapshot = await invoke<DesktopUpdaterSnapshot>("updater_check");
      applyDesktopUpdaterState(snapshot);
    }
  } catch (error) {
    checkFailed.value = true;
    updateError.value = errorText(error);
  } finally {
    checking.value = false;
    lastCheckedAt.value = Date.now();
  }
}

export async function applyUpdate(): Promise<void> {
  updateError.value = "";
  updating.value = true;
  updateProgress.value = 2;

  try {
    if (!isTauri) {
      if (!dockerUpdaterEnabled.value) {
        throw new Error(
          dockerMessage.value ||
            "Docker 一键更新未启用，请重新运行官网安装/迁移命令。",
        );
      }

      const before = await readDockerBuild();
      const targetRevision = lastDockerCheck?.target_revision || "";
      if (!before?.bootId || !targetRevision) {
        throw new Error("缺少当前 boot 或目标 OCI revision，请重新检查更新后再试。",);
      }

      dockerMessage.value = "正在把更新请求交给隔离更新服务…";
      let accepted: DockerUpdateAccepted | null = null;
      try {
        accepted = await invoke<DockerUpdateAccepted>("docker_update", { confirm: true });
      } catch (error) {
        // A structured HTTP failure means the server rejected the request before handoff.
        // Only a transport disconnect is ambiguous (the old container may have just exited).
        if (error instanceof BackendHttpError) throw error;
        dockerMessage.value = "连接已中断，正在确认容器是否已开始替换…";
      }

      if (accepted?.upToDate || accepted?.accepted === false) {
        updateVersion.value = null;
        upToDate.value = true;
        updateProgress.value = 100;
        dockerMessage.value = accepted.note || "当前容器已经是目标 build";
        return;
      }

      const resolvedTarget = accepted?.targetRevision || targetRevision;
      const resolvedBoot = accepted?.sourceBootId || before.bootId;
      if (!resolvedTarget || !resolvedBoot) {
        throw new Error("更新请求缺少目标 build 证据，已停止等待，请重新检查后再试。",);
      }
      dockerRequestId.value = accepted?.requestId || null;
      dockerMessage.value = accepted?.note || "更新请求已接单，正在拉取目标镜像…";
      updateProgress.value = 8;
      await waitForDockerReplacement({
        requestId: accepted?.requestId,
        sourceBootId: resolvedBoot,
        targetRevision: resolvedTarget,
        targetVersion: accepted?.targetVersion || lastDockerCheck?.latest,
        deadlineMs: accepted?.deadline
          ? accepted.deadline * 1000
          : Date.now() + DOCKER_UPDATE_TIMEOUT_MS,
      });
      return;
    }

    await ensureListener();
    await invoke("updater_apply");
  } catch (error) {
    dockerPollGeneration += 1;
    updateError.value = errorText(error);
    dockerMessage.value = updateError.value;
  } finally {
    updating.value = false;
    if (updateError.value) updateProgress.value = 0;
  }
}

export function dismissUpdate(): void {
  if (!updating.value) dialogDismissed.value = true;
}

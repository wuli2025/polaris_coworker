import { defineStore } from "pinia";
import { ref, computed } from "vue";
import {
  provider as providerApi,
  type ProviderView,
  type ProviderSaveInput,
  type UsageSummary,
  type CodexStatus,
  type CodexDeviceLogin,
  type CodexProxyInfo,
  type ClaudeAuthStatus,
  type ClaudeLoginStart,
  type ClaudeLoginResult,
} from "../tauri";

export const useProvidersStore = defineStore("providers", () => {
  const providers = ref<ProviderView[]>([]);
  const currentId = ref<string>("claude-official");
  const usage = ref<UsageSummary | null>(null);
  const codex = ref<CodexStatus | null>(null);
  const codexProxy = ref<CodexProxyInfo | null>(null);
  const claudeAuth = ref<ClaudeAuthStatus | null>(null);
  const loading = ref(false);
  const switching = ref<string | null>(null);
  const error = ref<string | null>(null);

  // 浮层开关
  const showAddModal = ref(false);
  const addTarget = ref<ProviderView | null>(null); // 预填的预设/待编辑供应商;null = 空白新建
  const showUsageBoard = ref(false);

  const current = computed(
    () => providers.value.find((p) => p.id === currentId.value) ?? null
  );

  function openAdd(target: ProviderView | null = null) {
    addTarget.value = target;
    showAddModal.value = true;
  }
  function closeAdd() {
    showAddModal.value = false;
    addTarget.value = null;
  }
  function openUsage() {
    showUsageBoard.value = true;
    refreshUsage();
  }
  function closeUsage() {
    showUsageBoard.value = false;
  }

  async function refresh() {
    loading.value = true;
    try {
      const res = await providerApi.list();
      providers.value = res.providers;
      currentId.value = res.currentId || "claude-official";
    } catch (e) {
      error.value = String(e);
    } finally {
      loading.value = false;
    }
  }

  async function refreshUsage() {
    try {
      usage.value = await providerApi.usage();
    } catch (e) {
      error.value = String(e);
    }
  }

  async function refreshCodex() {
    try {
      codex.value = await providerApi.codexStatus();
    } catch (e) {
      error.value = String(e);
    }
  }

  async function refreshCodexProxy() {
    try {
      codexProxy.value = await providerApi.codexProxyInfo();
    } catch (e) {
      error.value = String(e);
    }
  }

  /** ① 启动原生 Device Code 授权:后端会自动开浏览器,返回配对码供 UI 展示 */
  async function codexStartLogin(): Promise<CodexDeviceLogin | null> {
    error.value = null;
    try {
      return await providerApi.codexStartLogin();
    } catch (e) {
      error.value = String(e);
      return null;
    }
  }

  /** ② 轮询一次授权状态;成功(ok)时顺带刷新 codex 状态。抛错交给调用方处理 */
  async function codexPollLogin(
    deviceCode: string,
    userCode: string
  ): Promise<"pending" | "ok"> {
    const r = await providerApi.codexPollLogin(deviceCode, userCode);
    if (r.status === "ok") await refreshCodex();
    return r.status;
  }

  // ── Claude 官方订阅 OAuth(对话框内复刻 setup-token)──
  async function refreshClaudeAuth() {
    try {
      claudeAuth.value = await providerApi.claudeAuthStatus();
    } catch (e) {
      error.value = String(e);
    }
  }

  /** ① 取授权 URL(后端已暂存 PKCE,并尝试自动开浏览器) */
  async function claudeLoginStart(): Promise<ClaudeLoginStart | null> {
    error.value = null;
    try {
      return await providerApi.claudeLoginStart();
    } catch (e) {
      error.value = String(e);
      return null;
    }
  }

  /** ② 提交浏览器拿到的 code(CODE#STATE);成功落盘后刷新登录态。抛错交调用方处理 */
  async function claudeLoginSubmit(code: string): Promise<ClaudeLoginResult> {
    const r = await providerApi.claudeLoginSubmit(code);
    if (r.ok) await refreshClaudeAuth();
    return r;
  }

  /** 切换供应商；返回是否成功（失败时 error 已设置，常见为缺 key） */
  async function switchTo(id: string): Promise<boolean> {
    if (switching.value) return false; // 防双发:切换涉及写 settings.json
    error.value = null;
    switching.value = id;
    try {
      await providerApi.switch(id);
      currentId.value = id;
      return true;
    } catch (e) {
      error.value = String(e);
      return false;
    } finally {
      switching.value = null;
    }
  }

  async function save(input: ProviderSaveInput): Promise<string | null> {
    error.value = null;
    try {
      const id = await providerApi.save(input);
      await refresh();
      return id;
    } catch (e) {
      error.value = String(e);
      return null;
    }
  }

  async function remove(id: string) {
    error.value = null;
    try {
      await providerApi.delete(id);
      await refresh();
    } catch (e) {
      error.value = String(e);
    }
  }

  return {
    providers,
    currentId,
    usage,
    codex,
    codexProxy,
    claudeAuth,
    loading,
    switching,
    error,
    showAddModal,
    addTarget,
    showUsageBoard,
    current,
    openAdd,
    closeAdd,
    openUsage,
    closeUsage,
    refresh,
    refreshUsage,
    refreshCodex,
    refreshCodexProxy,
    codexStartLogin,
    codexPollLogin,
    refreshClaudeAuth,
    claudeLoginStart,
    claudeLoginSubmit,
    switchTo,
    save,
    remove,
  };
});

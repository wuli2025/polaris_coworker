import { defineStore } from "pinia";
import { ref, computed, watch } from "vue";
import { files as fc, invoke } from "../tauri";
import { useFileTasksStore, type FileTaskId } from "./fileTasks";

/**
 * 「让 AI 更懂你」引导向导的全局开关 + 知识库画像(个人 / 企业)+ 双档(快速 / 深度)。
 *
 * 向导本体常驻挂在 App.vue(不随视图切换销毁),靠这个 store 控制显隐 —— 于是扫描/归类
 * 跑着时用户可以「转入后台」隐掉浮层、最小化窗口、去逛别的视图,后台线程与事件监听照常推进,
 * 再点「智能向导」回来还停在原来那一步(状态不丢)。
 *
 * 画像(profile)决定走哪套知识构建方案(见桌面《聚类派 B 方案与框架派 D 方案改造报告》):
 *   - personal(个人 / C 端)→ B 方案 · 聚类驱动:自动发现主题、起中文名,让你「这就是我常用的文件」;
 *   - enterprise(企业 / B 端)→ D 方案 · Schema-Guided:先选行业框,在框内抽显式三元组,低幻觉可审计。
 *
 * 档位(mode)是 v2 双档扫盘的核心(见桌面《新用户引导系统设计报告 v2》):
 *   - fast(快速档)→ 全盘轻量扫 + 重要性(近期)排序 + 词法秒聚类 + FTS5 倒排,60 秒看到重点,纯本地零云;
 *   - deep(深度档)→ 一键切换后**全自动全流程**在后台跑到底:完整盘点 → 索引(FTS5+向量+IVF)→ 语义重聚 + AI 命名。
 *
 * 库态(libTier)= 当前知识库被加工到什么程度,主界面据此显示「速览版 / 升级中 / 精读版」徽标:
 *   - quick        速览版:只有快速档成果(词法簇 + 近期排序),搜得到、看得到重点;
 *   - deepRunning  正在后台深度精读;
 *   - deep         精读版:深度全流程已跑完,向量语义检索 + AI 命名就绪。
 */
export type KbProfile = "personal" | "enterprise";
export type ScanMode = "fast" | "deep";
export type LibTier = "none" | "quick" | "deepRunning" | "deep";

const PROFILE_KEY = "polaris.kbProfile.v1";
const SCHEMA_KEY = "polaris.kbSchema.v1";
const TIER_KEY = "polaris.libTier.v1";
// 「用户想深度精读、但还差一个嵌入模型」的武装标记。深度档的灵魂是**语义**那一步(向量),没模型
// 跑不出来 —— 所以约定:用户表达深度意向后,**只要配好模型就自动开跑**。持久化:哪怕这会儿不配、
// 下次开软件配好 key,也会自动补上深度精读。
const PENDING_KEY = "polaris.pendingDeep.v1";

function loadProfile(): KbProfile | null {
  try {
    const p = localStorage.getItem(PROFILE_KEY);
    if (p === "personal" || p === "enterprise") return p;
  } catch {
    /* storage 不可用 */
  }
  return null;
}
function loadTier(): LibTier {
  try {
    const t = localStorage.getItem(TIER_KEY);
    // 上次若停在「升级中」,重开按未完成处理 → 回落 quick(用户可重新发起深度精读)。
    if (t === "quick" || t === "deep") return t;
  } catch {
    /* ignore */
  }
  return "none";
}

export const useWizardStore = defineStore("wizard", () => {
  const open = ref(false);
  // 已选画像(null = 还没选过 → 向导首步让用户选)。持久化:下次开软件记得用户是个人还是企业。
  const profile = ref<KbProfile | null>(loadProfile());
  // 企业路径选定的行业 schema id(如 finance / medical / ecommerce …)。
  const schemaId = ref<string>(localStorage.getItem(SCHEMA_KEY) || "");
  // 双档:默认快速(给没耐心的用户「先看重点」)。
  const mode = ref<ScanMode>("fast");
  // 当前知识库加工程度(决定主界面徽标 + 升级入口)。
  const libTier = ref<LibTier>(loadTier());
  // 已武装:用户想深度精读、就等模型就绪。
  const pendingDeep = ref<boolean>(localStorage.getItem(PENDING_KEY) === "1");

  // 个人 → 聚类(B);企业 → 框架抽取(D)。
  const method = computed<"cluster" | "schema">(() =>
    profile.value === "enterprise" ? "schema" : "cluster",
  );

  function setProfile(p: KbProfile) {
    profile.value = p;
    try {
      localStorage.setItem(PROFILE_KEY, p);
    } catch {
      /* ignore */
    }
  }
  function setSchema(id: string) {
    schemaId.value = id;
    try {
      localStorage.setItem(SCHEMA_KEY, id);
    } catch {
      /* ignore */
    }
  }
  function setMode(m: ScanMode) {
    mode.value = m;
  }
  function setTier(t: LibTier) {
    libTier.value = t;
    try {
      // 只持久化「稳定态」;deepRunning 是瞬态,不写盘(重开当作未完成)。
      if (t === "quick" || t === "deep") localStorage.setItem(TIER_KEY, t);
    } catch {
      /* ignore */
    }
  }

  function armDeep() {
    pendingDeep.value = true;
    try {
      localStorage.setItem(PENDING_KEY, "1");
    } catch {
      /* ignore */
    }
  }
  function disarmDeep() {
    pendingDeep.value = false;
    try {
      localStorage.removeItem(PENDING_KEY);
    } catch {
      /* ignore */
    }
  }
  // 嵌入模型是否就绪(配了 key 的云服务商 / 装了本地模型)—— 深度档语义那一步的前提。
  async function embedReady(): Promise<boolean> {
    try {
      const ov: any = await invoke("file_overview", { root: null });
      return !!ov?.hasEmbedProvider;
    } catch {
      return false;
    }
  }

  function openWizard() {
    open.value = true;
  }
  function closeWizard() {
    open.value = false;
  }

  // ───────────────────────── 深度档 · 全自动全流程编排 ─────────────────────────
  //
  // 用户点一下「深度精读」→ 后台**自动按既定顺序**把整个库精读到极致,中途零人工:
  //   ① 完整盘点(full=true,逐目录重扫,补齐增量没察觉到的改动)
  //   ② 建索引(FTS5 倒排 + 向量嵌入滴灌 + 到阈值自动 IVF 优化)—— 检索能力先就位
  //   ③ 语义重聚 + AI 命名(smartCluster 全量:向量齐了把词法簇升级成语义簇并起亲切名)
  //
  // 全程走 fileTasks 全局后台任务 store → 右下角任务中心可见进度、可停;向导关掉、切界面照常跑。
  // 每一步靠监听**上一步的 done 事件**(fileTasks 已统一注册)推进,不靠用户点击。
  const deepRunning = ref(false);
  const deepStage = ref<"" | "inventory" | "index" | "cluster">("");
  const deepDoneTick = ref(0); // 自增 → 主界面/向导 watch 它弹「升级完成」提醒
  const deepFailed = ref(false);

  // 等某后台任务从「运行中」回到「空闲」(完成或失败);返回 true=成功完成,false=失败/取消。
  function waitForTaskIdle(tasks: ReturnType<typeof useFileTasksStore>, id: FileTaskId): Promise<boolean> {
    return new Promise((resolve) => {
      // 已经空闲(极快完成或根本没起来)→ 立刻按当前 failed 态返回。
      if (!tasks.running[id]) {
        resolve(!tasks.failed[id]);
        return;
      }
      const stop = watch(
        () => tasks.running[id],
        (now, prev) => {
          if (prev && !now) {
            stop();
            resolve(!tasks.failed[id]);
          }
        },
      );
    });
  }

  async function runDeepPipeline(roots: string[] = []) {
    if (deepRunning.value) return;
    const tasks = useFileTasksStore();
    await tasks.ensureListeners();
    deepRunning.value = true;
    deepFailed.value = false;
    setTier("deepRunning");
    try {
      // ① 完整盘点
      deepStage.value = "inventory";
      await tasks.startInventory(roots, [], true);
      if (!(await waitForTaskIdle(tasks, "inventory"))) throw new Error("盘点未完成");

      // ② 建索引(FTS5 + 向量 + IVF)—— 检索先就位
      deepStage.value = "index";
      await tasks.startIndex();
      if (!(await waitForTaskIdle(tasks, "index"))) throw new Error("索引未完成");

      // ③ 语义重聚 + AI 命名(向量齐了,smartCluster 全量跑 T2 把词法簇升级成语义簇)
      deepStage.value = "cluster";
      await tasks.startSmartCluster();
      await waitForTaskIdle(tasks, "cluster"); // 命名失败也优雅降级,不阻断收尾

      deepStage.value = "";
      setTier("deep");
      deepDoneTick.value++;
    } catch {
      // 任一硬环节失败:退回速览版,标记失败态,用户可重试(成果不丢)。
      deepFailed.value = true;
      deepStage.value = "";
      setTier("quick");
    } finally {
      deepRunning.value = false;
    }
  }

  // 用户主动要深度精读(intro 选深度档 / 点升级钮 / 徽标点深度):配好模型就立刻跑,没配就武装等模型。
  // 返回 true=已开跑;false=还差模型(已武装,调用方去引导配模型)。
  async function requestDeep(roots: string[] = []): Promise<boolean> {
    if (deepRunning.value || libTier.value === "deep") return true;
    setMode("deep");
    if (await embedReady()) {
      disarmDeep();
      runDeepPipeline(roots);
      return true;
    }
    armDeep();
    return false;
  }

  // 模型就绪检查点:若之前武装过(用户想深度)且现在模型已配好 → 自动补跑深度精读。
  // 调用时机:配完 key 之后、离开「感官 API」设置页、开机启动 —— 真正做到「配好就自动跑」。
  async function checkPendingDeep(roots: string[] = []): Promise<boolean> {
    if (!pendingDeep.value || deepRunning.value || libTier.value === "deep") return false;
    if (!(await embedReady())) return false;
    disarmDeep();
    setMode("deep");
    runDeepPipeline(roots);
    return true;
  }

  const deepStageLabel = computed(() => {
    switch (deepStage.value) {
      case "inventory":
        return "正在完整盘点你的全部磁盘…";
      case "index":
        return "正在建立全文 + 语义检索索引…";
      case "cluster":
        return "正在用 AI 重新读懂、归类、起名…";
      default:
        return deepRunning.value ? "正在深度精读…" : "";
    }
  });

  return {
    open,
    profile,
    schemaId,
    mode,
    libTier,
    pendingDeep,
    method,
    setProfile,
    setSchema,
    setMode,
    setTier,
    armDeep,
    disarmDeep,
    embedReady,
    openWizard,
    closeWizard,
    // 深度档全自动编排
    deepRunning,
    deepStage,
    deepStageLabel,
    deepDoneTick,
    deepFailed,
    runDeepPipeline,
    requestDeep,
    checkPendingDeep,
  };
});

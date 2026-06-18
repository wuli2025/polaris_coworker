<script setup lang="ts">
/**
 * 智能向导「让 AI 更懂你」· v2 双档扫盘 —— 60 秒把电脑里最重要的东西全找出来。
 *
 * 核心改造(见桌面《新用户引导系统设计报告 v2》):
 *   ① 进门只做一个友好二选一 —— ⚡快速找重点(推荐) / 🔬深度精读;不暴露任何技术词。
 *   ② 快速档:全盘轻量扫(含 NAS/系统盘,一个不落)→ 重点文件**边扫边往上冒**(近期活跃优先,
 *      这是零成本的「重要性」近似)→ 词法秒聚类星图爆炸 → 迷你检索框立刻能搜(FTS5/实时兜底,纯本地零云)。
 *   ③ 深度档:点一下即在**后台全自动全流程**跑到极致(完整盘点 → 索引 → 语义重聚+AI 命名),
 *      做完静默升级成「精读版」。深度永不阻塞:用户照常用快速档成果。详见 stores/wizard.ts:runDeepPipeline。
 *
 * 全程复用既有命令(零新后端轮子):
 *   盘点  fc.scanFolders / tasks.startInventory(fable:inventory)
 *   重点  fc.grid(sort:recent) 轮询 —— 近期活跃 = 重要性的零成本近似
 *   归类  fc.smartCluster(quick, file:cluster) / tasks.startSmartCluster
 *   检索  fc.search(grep, FTS5/实时兜底)
 *   深度  wiz.runDeepPipeline → tasks.startInventory(full)/startIndex/startSmartCluster 串行编排
 */
import { ref, reactive, computed, onBeforeUnmount, defineAsyncComponent, watch } from "vue";
import {
  Sparkles,
  FolderSearch,
  Wand2,
  Radar,
  Orbit,
  X,
  Check,
  KeyRound,
  Brain,
  FileText,
  FileType2,
  FileSpreadsheet,
  Image as ImageIcon,
  Film,
  Music,
  Archive,
  Code2,
  LoaderCircle,
  ChevronRight,
  Layers,
  Folder,
  User,
  Building2,
  Network,
  ShieldCheck,
  Zap,
  Microscope,
  Rocket,
  Gauge,
  Sunrise,
} from "@lucide/vue";
import {
  files as fc,
  artifacts as artifactsApi,
  listen,
  invoke,
  openUrl,
  type ScanRootInfo,
  type FileOverview,
  type FileCard,
} from "../tauri";
import { useAppStore } from "../stores/app";
import { useWorkflowsStore } from "../stores/workflows";
import { useWizardStore, type ScanMode } from "../stores/wizard";
import { useFileTasksStore } from "../stores/fileTasks";
// 星河图谱依赖 cytoscape(~562KB),只在「知识图谱」步骤(v-if 网住)才渲染 → 按需加载。
const KnowledgeGraph = defineAsyncComponent(() => import("./KnowledgeGraph.vue"));

const app = useAppStore();
const workflows = useWorkflowsStore();
const wiz = useWizardStore();
const tasks = useFileTasksStore();

type Step = "intro" | "profile" | "scope" | "scan" | "model" | "organize" | "graph" | "finish";
// 进度条按画像走两套用词:个人=聚类「智能归类」,企业=框架「构建体系」。
// 个人(快速/深度)收尾都先给「速览」成果,故不含 model 步;企业(D 方案)需先配嵌入再框内抽取。
const STEPS = computed<{ key: Step; label: string }[]>(() => {
  const enterprise = wiz.profile === "enterprise";
  if (enterprise) {
    return [
      { key: "scope", label: "盘点范围" },
      { key: "scan", label: "全盘扫描" },
      { key: "model", label: "配模型" },
      { key: "organize", label: "构建知识体系" },
      { key: "finish", label: "进对话" },
    ];
  }
  return [
    { key: "scope", label: "盘点范围" },
    { key: "scan", label: "找出重点" },
    { key: "organize", label: "智能归类" },
    { key: "graph", label: "知识图谱" },
    { key: "finish", label: "进对话" },
  ];
});
const step = ref<Step>("intro");
const stepIndex = computed(() => STEPS.value.findIndex((s) => s.key === step.value));

// ── 盘点范围 ──
const roots = ref<ScanRootInfo[]>([]);
const loadingRoots = ref(false);
const rootsErr = ref("");
const uncheckedRoots = reactive(new Set<string>());
const excludeKeywords = ref("");
// 快速档默认把盘符列表折叠起来(没耐心的用户一步都不用想,全盘默认全选直接开始);
// 想精挑或走深度档再展开。
const scopeExpanded = ref(false);
function loadRoots() {
  loadingRoots.value = true;
  rootsErr.value = "";
  fc.scanFolders(null)
    .then((r) => {
      roots.value = r.roots;
      uncheckedRoots.clear();
      // 默认全勾(后端现在「一个不落」把所有盘/卷 defaultOn=true);只把 defaultOn=false 的预置为不勾。
      for (const x of r.roots) if (!x.defaultOn) uncheckedRoots.add(x.path);
      // 后台逐个算每个盘/目录的真实体量(限并发,跟文件中心·盘点选择器一致)。
      sizeCache.clear();
      sizeQueue.length = 0;
      sizeInflight.clear();
      for (const x of r.roots) requestSize(x.path);
    })
    .catch((e: any) => (rootsErr.value = `扫描失败:${e?.message ?? e}`))
    .finally(() => (loadingRoots.value = false));
}

// ── 体积:限并发的后台计算队列(复用后端 fable_folder_size,带 10s 死线兜底)──
const sizeCache = reactive(new Map<string, { files: number; bytes: number }>());
const sizeQueue: string[] = [];
const sizeInflight = new Set<string>();
let sizeActive = 0;
const SIZE_CONCURRENCY = 4;
function requestSize(path: string) {
  if (sizeCache.has(path) || sizeInflight.has(path) || sizeQueue.includes(path)) return;
  sizeQueue.push(path);
  pumpSize();
}
function pumpSize() {
  while (sizeActive < SIZE_CONCURRENCY && sizeQueue.length) {
    const p = sizeQueue.shift()!;
    sizeInflight.add(p);
    sizeActive++;
    fc.folderSize(p)
      .then((r) => sizeCache.set(p, r))
      .catch(() => sizeCache.set(p, { files: 0, bytes: 0 }))
      .finally(() => {
        sizeActive--;
        sizeInflight.delete(p);
        pumpSize();
      });
  }
}
// 仿 WizTree:每个盘/目录在「所有范围」里的占比 + 条长(占比=本项/已知总和;条长=本项/已知最大)。
const rootSizeStats = computed(() => {
  let sum = 0;
  let max = 1;
  for (const r of roots.value) {
    const s = sizeCache.get(r.path);
    if (s) {
      sum += s.bytes;
      if (s.bytes > max) max = s.bytes;
    }
  }
  return { sum, max };
});
function rootSizePct(r: ScanRootInfo): number {
  const s = sizeCache.get(r.path);
  if (!s || rootSizeStats.value.sum <= 0) return 0;
  return (s.bytes / rootSizeStats.value.sum) * 100;
}
function rootSizeBar(r: ScanRootInfo): number {
  const s = sizeCache.get(r.path);
  if (!s || rootSizeStats.value.max <= 0) return 0;
  return s.bytes / rootSizeStats.value.max;
}
function fmtBytes(b: number): string {
  if (b <= 0) return "0 B";
  const u = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  let n = b;
  while (n >= 1024 && i < u.length - 1) {
    n /= 1024;
    i++;
  }
  return `${n >= 100 || i === 0 ? Math.round(n) : n.toFixed(1)} ${u[i]}`;
}
const totalScopeBytes = computed(() => {
  let sum = 0;
  for (const r of roots.value) if (isRootOn(r)) sum += sizeCache.get(r.path)?.bytes ?? 0;
  return sum;
});
const keywordList = computed(() =>
  excludeKeywords.value
    .split(/[,，;；\s]+/)
    .map((s) => s.trim().toLowerCase())
    .filter(Boolean),
);
function matchesKeyword(r: ScanRootInfo): boolean {
  if (!keywordList.value.length) return false;
  const hay = (r.path + " " + r.label).toLowerCase();
  return keywordList.value.some((k) => hay.includes(k));
}
function isRootOn(r: ScanRootInfo): boolean {
  return !uncheckedRoots.has(r.path) && !matchesKeyword(r);
}
function toggleRoot(r: ScanRootInfo) {
  if (uncheckedRoots.has(r.path)) uncheckedRoots.delete(r.path);
  else uncheckedRoots.add(r.path);
}
function selectAll(on: boolean) {
  uncheckedRoots.clear();
  if (!on) for (const r of roots.value) uncheckedRoots.add(r.path);
}
const selectedRoots = computed(() => roots.value.filter(isRootOn).map((r) => r.path));

// ── 扫描 + 重点边扫边冒 ──
const scanFiles = ref(0);
const scanMsg = ref("");
const scanFailed = ref(false);
const scanCancelled = ref(false);
// 「本向导发起的盘点还没被消费」的所有权标记 —— 深度档后台跑的 full 完整盘点也会发 fable:inventory
// done 事件,但那不属于向导流程,绝不能让它把用户从「收尾」拽回「归类」。只认自己发起的那一次。
const awaitingScanDone = ref(false);
let unScan: (() => void) | null = null;
// 重点文件实时列表:近期活跃 = 重要性的零成本近似(report 评分第一权重「+ 近期」)。
// 扫描中每隔约 1.4s 拉一次 fc.grid(sort:recent) → 新文件以打字机/滑入方式往上冒,等待变惊喜。
const hotFiles = ref<FileCard[]>([]);
let hotTimer: number | null = null;
async function pumpHot() {
  try {
    const page = await fc.grid({ sort: "recent", pageSize: 14 });
    if (step.value !== "scan") return;
    // 保持顺序稳定(按 path 去重),让 transition-group 只对真正新出现的项播入场动效。
    hotFiles.value = page.items.slice(0, 12);
  } catch {
    /* 扫描早期库还空、查询抖动都忽略 */
  }
}
function startHotPolling() {
  hotFiles.value = [];
  stopHotPolling();
  void pumpHot();
  hotTimer = window.setInterval(() => void pumpHot(), 1400);
}
function stopHotPolling() {
  if (hotTimer !== null) {
    window.clearInterval(hotTimer);
    hotTimer = null;
  }
  stopNarration();
}
const KIND_ICON: Record<string, any> = {
  text: FileText,
  doc: FileType2,
  image: ImageIcon,
  video: Film,
  audio: Music,
  archive: Archive,
  code: Code2,
  table: FileSpreadsheet,
};
function kindIcon(c: FileCard): any {
  return KIND_ICON[c.kind] ?? FileText;
}

// 扫描旁白轮播:等待不空白 —— 用第一人称讲「我正在为你做什么」,把干等变成期待(报告·哇时刻铺垫)。
const SCAN_NARRATION = [
  "正在读取你电脑里的每个文件…",
  "文档、图片、视频,正在分门别类…",
  "把文件的名字、时间、大小都记下来…",
  "正在挑出你最近在用的重点…",
  "马上就能看到 AI 眼里的你了…",
];
const narrIdx = ref(0);
let narrTimer: number | null = null;
function startNarration() {
  narrIdx.value = 0;
  if (narrTimer !== null) window.clearInterval(narrTimer);
  narrTimer = window.setInterval(() => {
    narrIdx.value = (narrIdx.value + 1) % SCAN_NARRATION.length;
  }, 2600);
}
function stopNarration() {
  if (narrTimer !== null) {
    window.clearInterval(narrTimer);
    narrTimer = null;
  }
}

// 哇时刻 1 · 星图字幕仪式:进知识图谱时,「这就是你的知识库 —— AI 自动分了 N 个主题」+ top-3 主题名错峰浮现,
// 数秒后自动淡出不挡操作。星图本身的入场靠 KnowledgeGraph 的 fcose 布局自然铺开,这里加「演出来」的字幕层。
const ceremonyStep = ref(0); // 0=未演 1=标题 2=副标+主题 3=淡出
const ceremonyThemes = computed(() =>
  (overview.value?.clusters ?? [])
    .filter((c) => c.parent === 0 && !!c.label)
    .slice(0, 3)
    .map((c) => c.label),
);
let ceremonyTimers: number[] = [];
function runCeremony() {
  ceremonyTimers.forEach((t) => window.clearTimeout(t));
  ceremonyTimers = [];
  ceremonyStep.value = 1;
  ceremonyTimers.push(window.setTimeout(() => (ceremonyStep.value = 2), 650));
  ceremonyTimers.push(window.setTimeout(() => (ceremonyStep.value = 3), 5400)); // 自动淡出
}
function clearCeremony() {
  ceremonyTimers.forEach((t) => window.clearTimeout(t));
  ceremonyTimers = [];
  ceremonyStep.value = 0;
}

async function startScan() {
  step.value = "scan";
  scanFiles.value = 0;
  scanFailed.value = false;
  scanCancelled.value = false;
  awaitingScanDone.value = true;
  scanMsg.value = "正在快速扫描你的全部磁盘,自动挑出重点…";
  startHotPolling();
  startNarration();
  try {
    if (!unScan) {
      unScan = await listen<{ kind: string; files?: number; message?: string }>(
        "fable:inventory",
        (p) => {
          // 已离开 scan 步:只有「本向导发起、尚未消费」的那次盘点完成才把流程往前推;
          // 深度档后台的 full 盘点事件在这里被忽略(awaitingScanDone 已是 false)。
          if (step.value !== "scan") {
            if (p.kind === "done" && awaitingScanDone.value) {
              awaitingScanDone.value = false;
              void afterScan();
            }
            return;
          }
          if (p.kind === "progress") {
            scanFailed.value = false;
            scanFiles.value = p.files ?? 0;
            scanMsg.value = `已找到 ${scanFiles.value.toLocaleString()} 个文件 · 仍在扫描…`;
          } else if (p.kind === "done") {
            awaitingScanDone.value = false;
            scanFiles.value = p.files ?? scanFiles.value;
            scanMsg.value = `扫描完成 · 共 ${scanFiles.value.toLocaleString()} 个文件`;
            void afterScan();
          } else if (p.kind === "error") {
            scanCancelled.value = (p.message ?? "").includes("已取消");
            scanFailed.value = true;
            scanMsg.value = scanCancelled.value ? "扫描已取消" : `扫描失败:${p.message ?? ""}`;
          }
        },
      );
    }
    // 向导里走「智能增量」(full=false)轻量快扫,给 60 秒看到重点的惊喜;深度档稍后在后台做 full 完整盘点。
    await tasks.startInventory(selectedRoots.value, [], false);
  } catch (e: any) {
    scanFailed.value = true;
    scanMsg.value = `扫描失败:${e?.message ?? e}`;
  }
}
function backToScope() {
  scanFailed.value = false;
  scanCancelled.value = false;
  stopHotPolling();
  step.value = "scope";
}
async function afterScan() {
  stopHotPolling();
  await loadOverview();
  await loadSense();
  // 企业(D 方案)需先配嵌入再框内抽取 → 进 model 步;个人(快速/深度)直接归类,不打断「找重点」的爽感。
  if (wiz.method === "schema") {
    step.value = "model";
  } else {
    void startOrganize();
  }
}

// ── 总览 ──
const overview = ref<FileOverview | null>(null);
async function loadOverview() {
  try {
    overview.value = await fc.overview(null);
  } catch {
    overview.value = null;
  }
}

// ── 模型 / 归类方式(仅企业 model 步用)──
const clusterWay = ref<"ai" | "offline">("ai");
function goConfigEmbed() {
  app.setView("sense_api");
  wiz.closeWizard();
}

// ── 前置「寓言计划」感官模型配置:嵌入 key(决定语义聚类/检索质量)+ 本地模型下载 ──
interface SenseProv {
  id: string;
  name: string;
  kind: string;
  key_ready: boolean;
  enabled: boolean;
  get_key_url: string;
  installed: boolean;
  pack_id: string | null;
  free: boolean;
}
const embedProv = ref<SenseProv | null>(null);
const localPacks = ref<SenseProv[]>([]);
const embedKey = ref("");
const embedSaving = ref(false);
const embedMsg = ref("");
const packPct = reactive<Record<string, number>>({});
let unPack: (() => void) | null = null;
async function loadSense() {
  try {
    const ov: any = await invoke("sense_list");
    const provs: SenseProv[] = (ov.groups || []).flatMap((g: any) => g.providers || []);
    embedProv.value = provs.find((p) => p.id === "siliconflow-embed") ?? null;
    localPacks.value = provs.filter((p) => p.kind === "local");
  } catch {
    /* 浏览器/降级模式忽略 */
  }
}
async function saveEmbedKey() {
  if (!embedProv.value || embedSaving.value || !embedKey.value.trim()) return;
  embedSaving.value = true;
  embedMsg.value = "";
  try {
    await invoke("sense_set", { id: embedProv.value.id, apiKey: embedKey.value.trim(), enabled: true });
    embedKey.value = "";
    await loadSense();
    await loadOverview();
    embedMsg.value = embedProv.value?.key_ready ? "已配置 ✓ 语义检索就绪" : "已保存";
    // 配好模型 = 深度档的前提齐了 → 若用户已表达深度意向(选了深度档 / 武装过),立刻自动开跑(配好就跑)。
    if (!wiz.deepRunning && wiz.libTier !== "deep" && (wiz.pendingDeep || wiz.mode === "deep")) {
      embedMsg.value = "已配置 ✓ 开始深度精读…";
      await wiz.requestDeep(selectedRoots.value);
    }
  } catch (e: any) {
    embedMsg.value = `保存失败:${e?.message ?? e}`;
  } finally {
    embedSaving.value = false;
  }
}
async function downloadPack(p: SenseProv) {
  if (!p.pack_id) return;
  if (!unPack) {
    unPack = await listen<{ id: string; kind: string; pct?: number; message?: string }>(
      "sense:pack",
      (ev) => {
        if (ev.kind === "progress" || ev.kind === "phase") {
          packPct[ev.id] = ev.pct ?? packPct[ev.id] ?? 0;
        } else {
          delete packPct[ev.id];
          void loadSense();
        }
      },
    );
  }
  packPct[p.pack_id] = 0;
  invoke("sense_pack_install", { id: p.pack_id }).catch(() => {
    if (p.pack_id) delete packPct[p.pack_id];
  });
}
function openKeyUrl() {
  if (embedProv.value?.get_key_url) openUrl(embedProv.value.get_key_url).catch(() => {});
}

// ── 归类 ──
const organizing = ref(false);
const organizeMsg = ref("");
let unCluster: (() => void) | null = null;
async function ensureClusterListener() {
  if (unCluster) return;
  unCluster = await listen<{ kind: string; text?: string; note?: string; message?: string }>(
    "file:cluster",
    (p) => {
      // 只在「归类」步消费 file:cluster —— 深度档后台的语义重聚(同走 file:cluster)在收尾/关闭态下
      // 触发,绝不能让它把用户从「收尾」拽回「知识图谱」。
      if (step.value !== "organize") return;
      if (p.kind === "phase") organizeMsg.value = p.text ?? organizeMsg.value;
      else if (p.kind === "tier") organizeMsg.value = p.note ?? organizeMsg.value;
      else if (p.kind === "done") {
        organizeMsg.value = p.note ?? "归类完成";
        void afterOrganize();
      } else if (p.kind === "error") {
        organizeMsg.value = `归类失败:${p.message ?? ""}`;
        organizing.value = false;
      }
    },
  );
}
async function runOfflineCluster() {
  await ensureClusterListener();
  await fc.clusterBuild(null);
}
// 企业路径(D 方案):Schema-Guided 在框内抽三元组,事件走 fable:ontology。
const ontoKept = ref(0);
let unOnto: (() => void) | null = null;
async function runSchemaExtract() {
  if (!unOnto) {
    unOnto = await listen<{ kind: string; text?: string; kept?: number; note?: string; message?: string }>(
      "fable:ontology",
      (p) => {
        if (p.kind === "phase") organizeMsg.value = p.text ?? organizeMsg.value;
        else if (p.kind === "tick") organizeMsg.value = "模型正在框内抽取关系…";
        else if (p.kind === "done") {
          ontoKept.value = p.kept ?? 0;
          organizeMsg.value = p.note ?? "知识体系构建完成";
          void afterOrganize();
        } else if (p.kind === "error") {
          organizeMsg.value = `构建失败:${p.message ?? ""}`;
          organizing.value = false;
        }
      },
    );
  }
  await tasks.startOntology(wiz.schemaId);
}
async function startOrganize() {
  step.value = "organize";
  organizing.value = true;
  // 归类要跑几秒,趁这空当把星河图谱(cytoscape ~562KB)的 chunk 预下载好。
  void import("./KnowledgeGraph.vue").catch(() => {});
  if (wiz.method === "schema") {
    organizeMsg.value = `正在按「${chosenSchema.value?.name ?? "行业框"}」抽取实体与关系…`;
    try {
      await runSchemaExtract();
    } catch (e: any) {
      organizeMsg.value = `构建失败:${e?.message ?? e}`;
      organizing.value = false;
    }
    return;
  }
  if (clusterWay.value === "offline") {
    organizeMsg.value = "正在按文件夹 / 名称离线归类(无需联网)…";
    try {
      await runOfflineCluster();
    } catch (e: any) {
      organizeMsg.value = `归类失败:${e?.message ?? e}`;
      organizing.value = false;
    }
    return;
  }
  // 快速档智能归类(v3 全覆盖):T0 词法把全库每个文件归类秒出星图 + T1 大模型读「簇画像」起亲切名。
  // quick=true 跳过耗时的 T2 全量向量化(深度档稍后在后台做)。AI 命名失败优雅降级保留词法名。
  organizeMsg.value = "正在快速归类你的全部文件、再请 AI 读懂起名…";
  try {
    await ensureClusterListener();
    await fc.smartCluster(null, true);
  } catch (e: any) {
    organizeMsg.value = `归类失败:${e?.message ?? e}`;
    organizing.value = false;
  }
}
async function afterOrganize() {
  await loadOverview();
  organizing.value = false;
  // 企业(D 方案)没有文件聚类星图,直接进收尾;个人(B 方案)先看知识图谱 + 星图字幕仪式(哇时刻 1)。
  step.value = wiz.method === "schema" ? "finish" : "graph";
  if (step.value === "finish") void finishUp();
  else runCeremony();
}

// ── 知识图谱 ──
const topThemeCount = computed(() => (overview.value?.clusters ?? []).filter((c) => c.parent === 0).length);

// ── 收尾:后台建索引 + 生成桌面画像 + 推荐工作流 + 深度档全自动编排 ──
const finishing = ref(false);
const profilePath = ref("");
const indexKicked = ref(false);
async function finishUp() {
  step.value = "finish";
  finishing.value = true;
  // 走到这一步至少已是「速览版」。
  if (wiz.libTier === "none") wiz.setTier("quick");
  // 深度档:用户在 intro 选了「深度精读」→ 配好模型就**自动**把全流程(含索引)交给后台编排;
  // 没配则武装(配好即自动跑)。先于建索引判断,好让下面知道深度全流程是否已接管索引。
  if (wiz.mode === "deep" && !wiz.deepRunning && wiz.libTier !== "deep") {
    const started = await wiz.requestDeep(selectedRoots.value);
    if (!started) await loadSense(); // 还差模型 → 收尾卡就地露出配 key 的入口
  }
  // 深度全流程没在跑时(快速档,或选了深度但还差模型)→ 后台建一份基础索引,让搜索先快起来,
  // 不至于卡在「武装等模型」期间连全文检索都没有。深度全流程在跑时它会按既定顺序统一编排,不重复起。
  if (!wiz.deepRunning) {
    tasks.startIndex().then(() => (indexKicked.value = true)).catch(() => {});
  }
  // 确定性生成桌面画像(秒级,不调大模型)。
  try {
    profilePath.value = await fc.profileHtml(null);
  } catch {
    profilePath.value = "";
  }
  finishing.value = false;
  // 大模型据真实知识库智能匹配收尾建议(数秒,后台跑;不阻塞「完成」按钮)。
  void loadSuggestions();
}
// 「升级深度精读」钩子(快速档收尾卡 / 主界面徽标都可触发):配好模型就一键后台全自动跑,没配就武装等模型。
async function upgradeToDeep() {
  const started = await wiz.requestDeep(selectedRoots.value);
  if (!started) await loadSense(); // 还差模型:已武装,露出就地配 key 的入口,保存后自动开跑
}
function openProfile() {
  if (profilePath.value) artifactsApi.openExternal(profilePath.value).catch(() => {});
}

interface FlowCard {
  title: string;
  why?: string;
  prompt: string;
}
const suggested = ref<FlowCard[]>([]);
const suggesting = ref(false);
// 只渲染「已浮现」的前 N 张 → transition-group 真正逐张播入场(v-show 不会触发入场过渡)。
const revealedFlows = computed(() => suggested.value.slice(0, flowsRevealed.value));
function localFallbackFlows(): FlowCard[] {
  const ov = overview.value;
  const cnt = (k: string) => ov?.byKind.find((x) => x.kind === k)?.count ?? 0;
  const out: FlowCard[] = [];
  if (cnt("video") >= 5)
    out.push({
      title: "把影视素材做成作品集",
      why: `你有 ${cnt("video")} 个视频`,
      prompt:
        "我电脑里有不少视频素材(已盘点进文件中心)。请帮我从中挑出代表作,配上简介与封面思路,生成一个可分享的作品集网页。先列出你打算收录哪些、为什么。",
    });
  if (cnt("doc") + cnt("text") >= 8)
    out.push({
      title: "为我的文档写一篇结构化总结",
      why: `你有 ${cnt("doc") + cnt("text")} 份文档`,
      prompt:
        "我的文档/资料已经盘点进知识库。请沿知识库通读后,按主题归纳要点、待办与关键结论,产出一份结构化总览(Markdown)。",
    });
  if (cnt("image") >= 20)
    out.push({
      title: "整理图片成图集",
      why: `你有 ${cnt("image")} 张图片`,
      prompt: "我有很多图片已盘点进文件中心。请按场景/时间帮我归类,挑出精选,排成一个图集页面。",
    });
  if (cnt("audio") >= 3)
    out.push({
      title: "把录音转写并归档",
      why: `你有 ${cnt("audio")} 段录音/音频`,
      prompt: "我有一些录音/音频已盘点进文件中心。请帮我规划把它们转写成文字、提炼摘要并沉淀进知识库的流程。",
    });
  if (!out.length)
    out.push({
      title: "从一个问题开始",
      why: `读完了你的 ${(ov?.totalFiles ?? 0).toLocaleString()} 个文件`,
      prompt: "看了我的文件,你觉得我最近在忙什么?请基于知识库给我三条具体的下一步建议。",
    });
  return out;
}
// 收尾「专属任务」哇时刻:逐张卡错峰浮现,营造「AI 想了想、专门为你列出来」的仪式感。
const flowsRevealed = ref(0);
let flowRevealTimer: number | null = null;
function staggerReveal(n: number) {
  if (flowRevealTimer !== null) window.clearInterval(flowRevealTimer);
  flowsRevealed.value = 0;
  flowRevealTimer = window.setInterval(() => {
    flowsRevealed.value++;
    if (flowsRevealed.value >= n && flowRevealTimer !== null) {
      window.clearInterval(flowRevealTimer);
      flowRevealTimer = null;
    }
  }, 220);
}
async function loadSuggestions() {
  suggesting.value = true;
  suggested.value = localFallbackFlows();
  staggerReveal(suggested.value.length);
  try {
    const flows = await fc.suggestWorkflows(null);
    if (flows && flows.length) {
      suggested.value = flows;
      staggerReveal(flows.length); // LLM 结果回来,再演一次浮现
    }
  } catch {
    /* 后端自身已会回落;真彻底失败就保留本地兜底卡 */
  } finally {
    suggesting.value = false;
  }
}
// 收尾「专属任务」个性化开场白:点名他真实的文件体量 + top 主题,让他一眼觉得「这是冲我来的」。
const suggestHeadline = computed(() => {
  const ov = overview.value;
  const n = ov?.totalFiles ?? 0;
  const themes = (ov?.clusters ?? []).filter((c) => c.parent === 0 && !!c.label).slice(0, 2).map((c) => c.label);
  if (n <= 0) return "我已经读完你的文件,这几件事最该现在做:";
  if (themes.length)
    return `我读完了你的 ${n.toLocaleString()} 个文件,看到你在忙「${themes.join("」「")}」—— 这几件事,我现在就能替你做:`;
  return `我读完了你的 ${n.toLocaleString()} 个文件 —— 这几件事,我现在就能替你做:`;
});
function useFlow(f: FlowCard) {
  app.setView("chat");
  window.setTimeout(() => workflows.insertText(f.prompt), 180);
  finishDone();
}

// ── 画像:个人(B 聚类)/ 企业(D Schema-Guided)──
interface SchemaCard {
  id: string;
  name: string;
  industry: string;
  source: string;
  desc: string;
  entities: { id: string; name: string; hint: string }[];
  relations: { id: string; name: string; hint: string }[];
  triples: number;
}
const schemaList = ref<SchemaCard[]>([]);
const schemasLoading = ref(false);
async function loadSchemas() {
  if (schemaList.value.length) return;
  schemasLoading.value = true;
  try {
    schemaList.value = await invoke<SchemaCard[]>("ontology_schemas");
  } catch {
    schemaList.value = [];
  } finally {
    schemasLoading.value = false;
  }
}
function pickPersonal() {
  wiz.setProfile("personal");
  step.value = "scope";
  loadRoots();
}
function pickEnterprise() {
  wiz.setProfile("enterprise");
  void loadSchemas();
}
function pickSchema(id: string) {
  wiz.setSchema(id);
}
function profileNext() {
  if (wiz.profile === "enterprise" && !wiz.schemaId) return;
  scopeExpanded.value = true; // 企业路径:直接展开完整盘符多选(框架抽取需精确范围)
  step.value = "scope";
  loadRoots();
}
const chosenSchema = computed(() => schemaList.value.find((s) => s.id === wiz.schemaId) || null);

// ── 双档:进门二选一 ──
function pickMode(m: ScanMode) {
  wiz.setMode(m);
  wiz.setProfile("personal");
  scopeExpanded.value = m === "deep"; // 深度档默认展开盘符精选;快速档折叠免纠结
  step.value = "scope";
  loadRoots();
}
// 企业 / 高级:落到原画像选择步(保留 D 方案能力)。
function goEnterprise() {
  step.value = "profile";
  if (wiz.profile === "enterprise") void loadSchemas();
}

// ── 生命周期 ──
// 转入后台 / X / 点遮罩:只隐藏浮层,**不动 step**,后台扫描/归类继续 —— 回来还在原地。
function close() {
  stopHotPolling();
  wiz.closeWizard();
}
// 「完成」/进对话:一段流程收尾,下次打开从头开始。
function finishDone() {
  stopHotPolling();
  wiz.closeWizard();
  step.value = "intro";
}
const inLongStep = computed(() => step.value === "scan" || step.value === "organize");
// 深度档跑完 → 若向导还开着,把库态徽标刷成「精读版」的提示由 finish 模板读 wiz 直接体现。
watch(
  () => wiz.deepDoneTick,
  () => {
    void loadOverview();
  },
);
onBeforeUnmount(() => {
  stopHotPolling();
  clearCeremony();
  if (flowRevealTimer !== null) window.clearInterval(flowRevealTimer);
  if (unScan) unScan();
  if (unCluster) unCluster();
  if (unPack) unPack();
  if (unOnto) unOnto();
});
</script>

<template>
  <transition name="wiz-fade">
    <div v-show="wiz.open" class="wiz-scrim" @click.self="close">
      <div class="wiz glass" :class="{ big: step === 'graph', intro: step === 'intro' }">
        <button class="wiz-x" :title="inLongStep ? '转入后台(任务继续跑)' : '关闭'" @click="close"><X :size="17" :stroke-width="2" /></button>

        <!-- 进度条(intro / profile 不显示) -->
        <div v-if="step !== 'intro' && step !== 'profile'" class="wiz-steps">
          <div
            v-for="(s, i) in STEPS"
            :key="s.key"
            class="ws"
            :class="{ on: i === stepIndex, done: i < stepIndex, fast: wiz.mode === 'fast', deep: wiz.mode === 'deep' }"
          >
            <span class="ws-dot"><Check v-if="i < stepIndex" :size="11" :stroke-width="3" /><template v-else>{{ i + 1 }}</template></span>
            <span class="ws-lab">{{ s.label }}</span>
          </div>
        </div>

        <!-- 0 · 欢迎 + 双档二选一 -->
        <div v-if="step === 'intro'" class="wiz-body intro-v2">
          <div class="intro-aura" />
          <span class="intro-badge"><span class="bd-dot" /> 北极星 · 让 AI 更懂你</span>
          <h2 class="intro-title">60&nbsp;秒,把你电脑里<br /><span class="grad">最重要的东西全找出来</span></h2>
          <p class="intro-lede">先选一种方式 —— 之后全程不用你操心,看着重点自己长出来。</p>

          <div class="lanes">
            <button class="lane f" @click="pickMode('fast')">
              <span class="lane-reco">推荐</span>
              <div class="lane-ic"><Zap :size="26" :stroke-width="1.7" /></div>
              <span class="lane-t">先快速找出我的重点</span>
              <span class="lane-s">全盘速扫,立刻用上</span>
              <ul class="lane-pts">
                <li><Check :size="13" :stroke-width="2.6" /> 含 NAS / 系统盘,一个不漏</li>
                <li><Check :size="13" :stroke-width="2.6" /> 重点边扫边往上冒</li>
                <li><Check :size="13" :stroke-width="2.6" /> 纯本地零云,断网也能用</li>
              </ul>
              <span class="lane-go">≤ 60 秒看到重点 <ChevronRight :size="15" :stroke-width="2" /></span>
            </button>

            <button class="lane d" @click="pickMode('deep')">
              <div class="lane-ic"><Microscope :size="26" :stroke-width="1.7" /></div>
              <span class="lane-t">我有时间,深度精读</span>
              <span class="lane-s">先给你重点,再后台精读到极致</span>
              <ul class="lane-pts">
                <li><Check :size="13" :stroke-width="2.6" /> 同样 60 秒先看到重点</li>
                <li><Check :size="13" :stroke-width="2.6" /> 后台全自动跑完整流程</li>
                <li><Check :size="13" :stroke-width="2.6" /> 连 NAS 老资料都搜得准</li>
              </ul>
              <span class="lane-go">一键后台精读 <ChevronRight :size="15" :stroke-width="2" /></span>
            </button>
          </div>

          <div class="intro-foot">
            <button class="intro-ent" @click="goEnterprise"><Building2 :size="13" :stroke-width="1.7" /> 这是公司 / 团队知识库?选行业框 →</button>
            <button class="wiz-skip" @click="close">跳过,稍后我自己来</button>
          </div>
        </div>

        <!-- 0.5 · 选画像:个人(B 聚类)/ 企业(D Schema) -->
        <div v-else-if="step === 'profile'" class="wiz-body">
          <h3 class="wiz-h"><Sparkles :size="16" :stroke-width="1.7" /> 先告诉我:这是谁的知识库?</h3>
          <p class="wiz-tip">我会据此**自动匹配**最合适的构建方案 —— 不用你懂技术。</p>
          <div class="prof-cards">
            <button class="prof-card" :class="{ on: wiz.profile === 'personal' }" @click="pickPersonal">
              <div class="pc-ic"><User :size="22" :stroke-width="1.6" /></div>
              <span class="pc-t">个人知识库</span>
              <span class="pc-d">我的电脑、我的资料。自动发现「我常用的几摊事」,起好中文名,一眼认出。</span>
              <span class="pc-tag">聚类驱动 · 自动发现</span>
            </button>
            <button class="prof-card" :class="{ on: wiz.profile === 'enterprise' }" @click="pickEnterprise">
              <div class="pc-ic"><Building2 :size="22" :stroke-width="1.6" /></div>
              <span class="pc-t">企业知识库</span>
              <span class="pc-d">公司 / 行业资料。先选行业框,在框内抽出实体与关系,每条可溯源、可审计。</span>
              <span class="pc-tag">框架派 · 低幻觉可审计</span>
            </button>
          </div>

          <div v-if="wiz.profile === 'enterprise'" class="schema-box">
            <div class="schema-head"><Network :size="14" :stroke-width="1.7" /> <b>选一个行业框</b><span class="schema-fine">框里只用给定的实体/关系类型,模型「做选择题」→ 不乱编</span></div>
            <div v-if="schemasLoading" class="wiz-loading"><LoaderCircle :size="16" class="spin" /> 加载行业框…</div>
            <div v-else class="schema-grid">
              <button
                v-for="sc in schemaList"
                :key="sc.id"
                class="schema-card"
                :class="{ on: wiz.schemaId === sc.id }"
                @click="pickSchema(sc.id)"
              >
                <span class="sc-name">{{ sc.name }}</span>
                <span class="sc-ind">{{ sc.industry }}</span>
                <span class="sc-types">{{ sc.entities.slice(0, 5).map((e) => e.name).join(' · ') }}</span>
                <span class="sc-src">源自 {{ sc.source }}</span>
              </button>
            </div>
            <div v-if="chosenSchema" class="schema-preview">
              <ShieldCheck :size="13" :stroke-width="1.8" />
              <span>「{{ chosenSchema.name }}」框内:<b>{{ chosenSchema.entities.length }}</b> 类实体、<b>{{ chosenSchema.relations.length }}</b> 类关系 —— {{ chosenSchema.relations.slice(0, 5).map((r) => r.name).join('、') }}</span>
            </div>
          </div>

          <div class="wiz-foot end">
            <button v-if="wiz.profile" class="wiz-go" :disabled="wiz.profile === 'enterprise' && !wiz.schemaId" @click="profileNext">
              <ChevronRight :size="15" :stroke-width="1.8" /> 下一步:盘点资料
            </button>
          </div>
        </div>

        <!-- 1 · 盘点范围 -->
        <div v-else-if="step === 'scope'" class="wiz-body">
          <h3 class="wiz-h">
            <component :is="wiz.method === 'schema' ? Network : wiz.mode === 'deep' ? Microscope : Zap" :size="16" :stroke-width="1.7" :class="wiz.method === 'schema' ? '' : wiz.mode === 'deep' ? 'ic-deep' : 'ic-fast'" />
            {{ wiz.method === 'schema' ? '盘点范围' : wiz.mode === 'deep' ? '深度精读 · 盘点范围' : '快速找重点 · 全盘速扫' }}
          </h3>
          <div v-if="loadingRoots" class="wiz-loading"><LoaderCircle :size="18" class="spin" /> 正在列出可盘点的盘…</div>
          <div v-else-if="rootsErr" class="wiz-err">{{ rootsErr }}</div>
          <template v-else>
            <!-- 快速档默认折叠:一句话承诺 + 一个开始按钮,没耐心的用户一步都不用想 -->
            <div v-if="!scopeExpanded" class="scope-quick">
              <div class="sq-card">
                <div class="sq-ic"><FolderSearch :size="22" :stroke-width="1.5" /></div>
                <div class="sq-txt">
                  <b>将快速扫描你的全部磁盘,自动挑出重点</b>
                  <span>共 {{ roots.length }} 个盘 / 目录,含 NAS、系统盘 —— 一个都不漏。系统、缓存目录会自动跳过。</span>
                </div>
                <span v-if="totalScopeBytes > 0" class="sq-size">{{ fmtBytes(totalScopeBytes) }}</span>
              </div>
              <button class="scope-tweak" @click="scopeExpanded = true"><Layers :size="13" :stroke-width="1.7" /> 想精挑要扫哪些盘?展开</button>
            </div>

            <!-- 展开:完整盘符多选(深度档默认 / 快速档点开) -->
            <template v-else>
              <p class="wiz-tip">已<b>默认全选</b>所有可访问的盘 / 目录。取消勾选可缩小范围,或填关键词排除。</p>
              <div class="root-tools">
                <button class="mini" @click="selectAll(true)">全选</button>
                <button class="mini" @click="selectAll(false)">全不选</button>
                <input v-model="excludeKeywords" class="kw" placeholder="排除含这些词的目录(逗号分隔,如 备份, temp)" />
              </div>
              <div class="root-list">
                <button
                  v-for="r in roots"
                  :key="r.path"
                  class="root-row"
                  :class="{ off: !isRootOn(r) }"
                  @click="toggleRoot(r)"
                >
                  <span class="root-check" :class="{ on: isRootOn(r) }"><Check v-if="isRootOn(r)" :size="12" :stroke-width="2.6" /></span>
                  <Layers v-if="r.defaultOn" :size="15" :stroke-width="1.7" class="root-ic" />
                  <Folder v-else :size="15" :stroke-width="1.7" class="root-ic" />
                  <span class="root-name">{{ r.label }}</span>
                  <span class="root-path">{{ r.path }}</span>
                  <template v-if="sizeCache.get(r.path)">
                    <span class="root-bar" :title="rootSizePct(r).toFixed(1) + '% 占总量'">
                      <span class="root-bar-fill" :style="{ width: Math.max(2, rootSizeBar(r) * 100) + '%' }" />
                    </span>
                    <span class="root-size">{{ fmtBytes(sizeCache.get(r.path)!.bytes) }}</span>
                  </template>
                  <span v-else class="root-calc"><LoaderCircle :size="11" class="spin" /> 计算体积…</span>
                </button>
              </div>
            </template>
          </template>
          <div class="wiz-foot">
            <span class="foot-count">已选 <b>{{ selectedRoots.length }}</b> 个范围</span>
            <button class="wiz-go" :class="{ deep: wiz.method !== 'schema' && wiz.mode === 'deep' }" :disabled="loadingRoots || !selectedRoots.length" @click="startScan">
              <FolderSearch :size="15" :stroke-width="1.8" /> {{ wiz.method === 'schema' ? '开始盘点' : wiz.mode === 'deep' ? '开始(先速览,后台精读)' : '开始找重点' }}
            </button>
          </div>
        </div>

        <!-- 2 · 扫描中:重点边扫边冒 -->
        <div v-else-if="step === 'scan'" class="wiz-body">
          <!-- 失败 / 取消 -->
          <template v-if="scanFailed">
            <div class="scan-center">
              <div class="scan-orb stopped"><component :is="scanCancelled ? Layers : X" :size="30" :stroke-width="1.4" /></div>
              <div class="scan-lab big">{{ scanMsg }}</div>
              <div class="scan-fine">{{ scanCancelled ? "盘点已停止。你可以重新扫描,或返回调整盘点范围。" : "扫描中断了。可重试,或返回重新选择范围。" }}</div>
              <div class="scan-actions">
                <button class="wiz-go ghost" @click="backToScope"><ChevronRight :size="14" :stroke-width="1.8" class="flip" /> 返回选范围</button>
                <button class="wiz-go" @click="startScan"><FolderSearch :size="15" :stroke-width="1.8" /> 重新扫描</button>
              </div>
            </div>
          </template>
          <!-- 进行中:左侧脉冲数字 + 右侧重点实时列表 -->
          <template v-else>
            <div class="scan-live">
              <div class="scan-head">
                <div class="scan-orb"><FolderSearch :size="24" :stroke-width="1.3" /><div class="scan-ring" /></div>
                <div class="scan-meta">
                  <div class="scan-num">{{ scanFiles.toLocaleString() }}<span class="scan-num-u">个文件</span></div>
                  <transition name="narr" mode="out-in">
                    <div :key="narrIdx" class="scan-narr">{{ SCAN_NARRATION[narrIdx] }}</div>
                  </transition>
                </div>
              </div>
              <div class="hot-title"><Sparkles :size="13" :stroke-width="1.8" /> 已经替你挑出的重点 <span class="hot-live">实时</span></div>
              <transition-group name="hot" tag="div" class="hot-list">
                <div v-for="f in hotFiles" :key="f.path" class="hot-row">
                  <span class="hot-ic" :class="'k-' + f.kind"><component :is="kindIcon(f)" :size="15" :stroke-width="1.7" /></span>
                  <span class="hot-name" :title="f.path">{{ f.title || f.name }}</span>
                  <span class="hot-meta">{{ f.sizeH }}</span>
                </div>
                <div v-if="!hotFiles.length" key="__empty" class="hot-empty"><LoaderCircle :size="13" class="spin" /> 正在你的盘里找重点…</div>
              </transition-group>
            </div>
            <div class="scan-foot">
              <button class="wiz-go ghost bg" @click="close"><Layers :size="14" :stroke-width="1.8" /> 转入后台 · 去逛别处</button>
              <span class="scan-fine">扫描在后台继续,可最小化窗口或去用别的功能;扫完再点「智能向导」回来。</span>
            </div>
          </template>
        </div>

        <!-- 3 · 配模型(仅企业)-->
        <div v-else-if="step === 'model'" class="wiz-body">
          <h3 class="wiz-h">让 AI 怎么理解你的文件</h3>
          <p class="wiz-tip">扫到 <b>{{ (overview?.totalFiles ?? 0).toLocaleString() }}</b> 个文件。配好嵌入模型,下一步在行业框内抽取实体与关系。</p>
          <div class="sense-box">
            <div class="sense-head">
              <Radar :size="14" :stroke-width="1.7" />
              <b>配模型(强烈建议)</b>
              <button class="sense-more" title="打开完整的「寓言计划 · 感官 API」设置页" @click="goConfigEmbed">完整设置↗</button>
              <span v-if="overview?.hasEmbedProvider" class="sense-ok"><Check :size="12" :stroke-width="2.6" /> 嵌入已就绪</span>
              <span v-else class="sense-warn">不配只能按文件名粗分 · 语义聚类 / 检索效果差很多</span>
            </div>
            <div v-if="embedProv" class="sense-row">
              <span class="sr-name">
                <span class="sr-dot" :class="{ on: embedProv.key_ready }" />
                {{ embedProv.name }}<em v-if="embedProv.free">免费</em>
              </span>
              <template v-if="embedProv.key_ready">
                <span class="sr-ready"><Check :size="12" :stroke-width="2.6" /> 已配置</span>
              </template>
              <template v-else>
                <input v-model="embedKey" type="password" class="sr-key" placeholder="粘贴硅基流动 API Key(sk-…)" />
                <button class="mini" @click="openKeyUrl"><KeyRound :size="12" :stroke-width="1.8" /> 获取 key↗</button>
                <button class="mini key" :disabled="embedSaving || !embedKey.trim()" @click="saveEmbedKey">
                  {{ embedSaving ? "保存中" : "保存" }}
                </button>
              </template>
              <span v-if="embedMsg" class="sr-msg">{{ embedMsg }}</span>
            </div>
            <div v-for="p in localPacks" :key="p.id" class="sense-row">
              <span class="sr-name">
                <span class="sr-dot" :class="{ on: p.installed }" />
                {{ p.name }}<em>本地</em>
              </span>
              <span v-if="p.installed" class="sr-ready"><Check :size="12" :stroke-width="2.6" /> 已下载</span>
              <template v-else-if="p.pack_id && packPct[p.pack_id] !== undefined">
                <span class="sr-prog"><span class="sr-fill" :style="{ width: packPct[p.pack_id] + '%' }" /></span>
                <span class="sr-msg">{{ packPct[p.pack_id] }}%</span>
              </template>
              <button v-else class="mini" @click="downloadPack(p)"><FileText :size="12" :stroke-width="1.8" /> 下载模型</button>
            </div>
          </div>
          <div class="wiz-foot end">
            <button v-if="!overview?.hasEmbedProvider" class="wiz-go ghost bg" @click="startOrganize">先跳过,直接构建</button>
            <button class="wiz-go" @click="startOrganize"><Network :size="15" :stroke-width="1.8" /> 开始构建知识体系</button>
          </div>
        </div>

        <!-- 4 · 归类中 / 构建中 -->
        <div v-else-if="step === 'organize'" class="wiz-body center">
          <div class="scan-orb"><component :is="wiz.method === 'schema' ? Network : Wand2" :size="28" :stroke-width="1.3" /><div class="scan-ring" /></div>
          <div class="scan-lab big">{{ organizeMsg }}</div>
          <div class="scan-fine">{{ wiz.method === 'schema' ? '模型正在行业框内抽实体与关系,只抽资料里写明的、可溯源的,稍候…' : 'AI 正在读你的文件清单分主题,马上把星图画出来…' }}</div>
          <button class="wiz-go ghost bg" @click="close"><Layers :size="14" :stroke-width="1.8" /> 转入后台 · 去逛别处</button>
        </div>

        <!-- 5 · 知识图谱:星图爆炸「哇时刻 1」 -->
        <div v-else-if="step === 'graph'" class="wiz-body">
          <h3 class="wiz-h"><Orbit :size="16" :stroke-width="1.7" /> 这是 AI 眼里的你</h3>
          <p class="wiz-tip">把你的文件归成了 <b>{{ topThemeCount }}</b> 个大主题。拖一拖、缩放看看 —— 下一步我会据这些,给你列几件**专属于你**的事。</p>
          <div class="graph-wrap reveal-pop">
            <KnowledgeGraph source="files" :embedded="true" />
            <!-- 哇时刻 1:星图字幕仪式,数秒后自动淡出不挡操作 -->
            <transition name="cer">
              <div v-if="ceremonyStep >= 1 && ceremonyStep < 3" class="cer-overlay">
                <div class="cer-card">
                  <div class="cer-h">这就是你的知识库</div>
                  <transition name="cer-sub">
                    <div v-if="ceremonyStep >= 2" class="cer-sub">
                      AI 自动分了 <b>{{ topThemeCount }}</b> 个主题
                      <div v-if="ceremonyThemes.length" class="cer-themes">
                        <span v-for="(t, i) in ceremonyThemes" :key="t" class="cer-chip" :style="{ animationDelay: 0.15 + i * 0.18 + 's' }">{{ t }}</span>
                      </div>
                    </div>
                  </transition>
                </div>
              </div>
            </transition>
          </div>
          <div class="wiz-foot end">
            <button class="wiz-go" @click="finishUp"><ChevronRight :size="15" :stroke-width="1.8" /> 继续:看我为你列的专属任务</button>
          </div>
        </div>

        <!-- 6 · 收尾 -->
        <div v-else-if="step === 'finish'" class="wiz-body">
          <h3 class="wiz-h"><Sparkles :size="16" :stroke-width="1.7" /> 全部就绪 · AI 已经懂你了</h3>
          <p v-if="wiz.method === 'schema'" class="wiz-tip">
            已在「<b>{{ chosenSchema?.name ?? '行业' }}</b>」框内抽出 <b>{{ ontoKept }}</b> 条可溯源的实体关系,存进了知识体系(可在「核心层」查看)。挑一件我能立刻替你做的事:
          </p>
          <!-- ⭐ 哇时刻:据真实文件给「独属于你」的任务,逐张错峰浮现 -->
          <p v-else class="suggest-headline"><Sparkles :size="15" :stroke-width="1.8" class="sh-ic" /> {{ suggestHeadline }}</p>

          <transition-group name="flow" tag="div" class="flow-cards hero">
            <button v-for="(f, i) in revealedFlows" :key="f.title + i" class="flow-card" @click="useFlow(f)">
              <div class="fc-main">
                <span class="fc-t">{{ f.title }}</span>
                <span v-if="f.why" class="fc-why"><Sparkles :size="11" :stroke-width="1.9" /> {{ f.why }}</span>
              </div>
              <span class="fc-do">交给我 <ChevronRight :size="14" :stroke-width="2" /></span>
            </button>
          </transition-group>
          <div v-if="suggesting" class="suggest-busy">
            <LoaderCircle :size="12" class="spin" /> AI 正在翻你的资料、为你量身想…(先点上面任意一条也行)
          </div>
          <button v-else class="suggest-redo" @click="loadSuggestions">
            <Sparkles :size="12" :stroke-width="1.8" /> 不够贴?让 AI 再想几条
          </button>

          <!-- 深度精读升级钩子(个人路径)-->
          <div v-if="wiz.method !== 'schema'" class="deep-hook" :class="{ running: wiz.deepRunning, done: wiz.libTier === 'deep' }">
            <div class="dh-ic">
              <component :is="wiz.libTier === 'deep' ? ShieldCheck : wiz.deepRunning ? Gauge : Rocket" :size="20" :stroke-width="1.7" />
            </div>
            <div class="dh-body">
              <template v-if="wiz.libTier === 'deep'">
                <b class="dh-t">已升级为「精读版」</b>
                <span class="dh-d">连 NAS 上的老资料都搜得准了 —— 向量语义检索 + AI 命名已就绪。</span>
              </template>
              <template v-else-if="wiz.deepRunning">
                <b class="dh-t"><LoaderCircle :size="13" class="spin" /> 正在后台深度精读…</b>
                <span class="dh-d">{{ wiz.deepStageLabel }} 放着不管就行,做完会自动升级,右下角任务中心可看进度。</span>
              </template>
              <!-- 已武装但还差模型:就地配 key,保存后**自动开跑**(只要配好模型就跑) -->
              <template v-else-if="wiz.pendingDeep && !overview?.hasEmbedProvider">
                <b class="dh-t">就差一个模型 —— 配好就<span class="tier-deep">立刻自动精读</span></b>
                <span class="dh-d">深度精读靠语义模型读懂每个文件的意思。粘贴免费的硅基流动 key,保存后我马上在后台跑完整流程。</span>
                <div v-if="embedProv && !embedProv.key_ready" class="dh-key">
                  <input v-model="embedKey" type="password" placeholder="粘贴硅基流动 API Key(sk-…)" @keydown.enter="saveEmbedKey" />
                  <button class="mini" @click="openKeyUrl"><KeyRound :size="12" :stroke-width="1.8" /> 获取 key↗</button>
                  <button class="mini key" :disabled="embedSaving || !embedKey.trim()" @click="saveEmbedKey">{{ embedSaving ? "保存中" : "保存并开始" }}</button>
                </div>
                <span v-if="embedMsg" class="dh-msg">{{ embedMsg }}</span>
              </template>
              <template v-else>
                <b class="dh-t">这是<span class="tier-quick">速览版</span> —— 重点都在、搜得到</b>
                <span class="dh-d">想让我读懂每个文件的意思、连 NAS 老资料都搜得准吗?点一下,后台慢慢精读,做完通知你。</span>
              </template>
            </div>
            <button
              v-if="wiz.libTier !== 'deep' && !wiz.deepRunning && !(wiz.pendingDeep && !overview?.hasEmbedProvider)"
              class="dh-btn"
              @click="upgradeToDeep"
            >
              <Microscope :size="14" :stroke-width="1.8" /> 升级深度精读
            </button>
          </div>

          <!-- 哇时刻 3:明早晨报长期钩子 —— 给用户「明天还想打开」的理由 -->
          <div class="briefing-hook">
            <div class="bh-ic"><Sunrise :size="20" :stroke-width="1.7" /></div>
            <div class="bh-body">
              <b class="bh-t">明早 8 点,我会主动找你</b>
              <span class="bh-d">我会在后台慢慢读懂你的资料,明早给你一份「昨天在忙什么 + 今天可以做什么」的晨报 —— 不用你开口。</span>
            </div>
          </div>

          <div class="finish-row">
            <button class="mini" :disabled="!profilePath" @click="openProfile"><FileText :size="12" :stroke-width="1.8" /> 打开桌面画像</button>
            <span v-if="finishing" class="fine-busy"><LoaderCircle :size="12" class="spin" /> 生成画像中…</span>
            <span v-else-if="profilePath" class="fine-ok"><Check :size="12" :stroke-width="2.4" /> 画像已存桌面</span>
          </div>
          <div class="wiz-foot end">
            <button class="wiz-go ghost" @click="finishDone">完成</button>
          </div>
        </div>
      </div>
    </div>
  </transition>
</template>

<style scoped>
/* 双档强调色(局部令牌,两套主题都用同一组,确保绿/紫的「快/深」语义一致) */
.wiz {
  --fast: #1fb573;
  --fast-2: #34d399;
  --deep: #7c6cff;
  --deep-2: #9d8cff;
}
.wiz-scrim {
  position: fixed;
  inset: 0;
  z-index: 80;
  display: flex;
  align-items: center;
  justify-content: center;
  background: color-mix(in srgb, var(--bg) 62%, transparent);
  -webkit-backdrop-filter: blur(6px);
  backdrop-filter: blur(6px);
  padding: 24px;
}
.glass {
  background: color-mix(in srgb, var(--panel) 78%, transparent);
  -webkit-backdrop-filter: blur(26px) saturate(1.5);
  backdrop-filter: blur(26px) saturate(1.5);
  border: 1px solid var(--border-soft);
  border-radius: 18px;
}
.wiz {
  position: relative;
  width: min(1040px, 94vw);
  max-height: 92vh;
  overflow: hidden;
  display: flex;
  flex-direction: column;
  box-shadow: var(--shadow-lg, 0 24px 80px rgba(0, 0, 0, 0.4));
  transition: width 0.2s ease;
}
.wiz.intro { width: min(880px, 94vw); }
.wiz.big { width: min(1280px, 97vw); }
.wiz-x {
  position: absolute;
  top: 14px;
  right: 14px;
  z-index: 3;
  display: inline-flex;
  border: none;
  background: transparent;
  color: var(--dim);
  cursor: pointer;
  padding: 4px;
  border-radius: 8px;
}
.wiz-x:hover { color: var(--text); background: var(--selection-bg); }

/* 步骤条 */
.wiz-steps {
  display: flex;
  gap: 4px;
  padding: 16px 22px 12px;
  border-bottom: 1px solid var(--border-soft);
  overflow-x: auto;
}
.ws { display: flex; align-items: center; gap: 7px; padding: 0 10px 0 0; opacity: 0.5; flex: none; }
.ws.on, .ws.done { opacity: 1; }
.ws-dot {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  border-radius: 50%;
  border: 1px solid var(--border-strong);
  font-size: 11px;
  color: var(--muted);
  flex: none;
  transition: background 0.2s, border-color 0.2s, color 0.2s;
}
.ws.on.fast .ws-dot { background: var(--fast); border-color: var(--fast); color: #06281b; }
.ws.done.fast .ws-dot { background: color-mix(in srgb, var(--fast) 22%, transparent); border-color: var(--fast); color: var(--fast); }
.ws.on.deep .ws-dot { background: var(--deep); border-color: var(--deep); color: #fff; }
.ws.done.deep .ws-dot { background: color-mix(in srgb, var(--deep) 22%, transparent); border-color: var(--deep); color: var(--deep-2); }
.ws-lab { font-size: 12px; color: var(--text-2); white-space: nowrap; }
.ws.on .ws-lab { color: var(--text); font-weight: 600; }

/* 主体 */
.wiz-body { padding: 22px 26px 24px; overflow-y: auto; }
.wiz-body.center { display: flex; flex-direction: column; align-items: center; justify-content: center; text-align: center; min-height: 320px; gap: 14px; }
.wiz-h { display: flex; align-items: center; gap: 8px; font-size: 16px; margin: 0 0 8px; color: var(--ink); }
.wiz-h .ic-fast { color: var(--fast); }
.wiz-h .ic-deep { color: var(--deep-2); }
.wiz-tip { font-size: 13px; color: var(--muted); line-height: 1.6; margin: 0 0 16px; }
.wiz-tip b, .wiz-fine b { color: var(--text); }
.wiz-fine { font-size: 12px; color: var(--dim); margin: 10px 0 0; }
.wiz-loading, .wiz-err { display: flex; align-items: center; gap: 8px; font-size: 13px; color: var(--muted); padding: 20px 0; }
.wiz-err { color: var(--danger, #e0736b); }

/* ───────── 欢迎 v2:双档二选一 ───────── */
.intro-v2 { position: relative; text-align: center; padding: 40px 38px 30px; overflow: hidden; }
.intro-aura {
  position: absolute; inset: -40% -20% auto; height: 380px; z-index: 0; pointer-events: none;
  background:
    radial-gradient(46% 60% at 30% 0%, color-mix(in srgb, var(--fast) 26%, transparent), transparent 70%),
    radial-gradient(46% 60% at 72% 6%, color-mix(in srgb, var(--deep) 26%, transparent), transparent 70%);
  filter: blur(36px); opacity: 0.7;
}
.intro-v2 > * { position: relative; z-index: 1; }
.intro-badge {
  display: inline-flex; align-items: center; gap: 8px;
  padding: 6px 14px; margin-bottom: 18px;
  border: 1px solid var(--border-soft); border-radius: 99px;
  background: color-mix(in srgb, var(--panel) 60%, transparent);
  font-size: 12px; color: var(--text-2); letter-spacing: 0.4px;
}
.bd-dot { width: 7px; height: 7px; border-radius: 50%; background: var(--fast-2); box-shadow: 0 0 10px var(--fast-2); }
.intro-title { font-family: var(--serif); font-size: 30px; line-height: 1.25; font-weight: 700; letter-spacing: 0.5px; margin: 0 0 12px; color: var(--ink); }
.intro-title .grad {
  background: linear-gradient(110deg, var(--fast-2), #5fd0e0 52%, var(--deep-2));
  -webkit-background-clip: text; background-clip: text; color: transparent;
}
.intro-lede { font-size: 14px; color: var(--muted); max-width: 560px; margin: 0 auto 28px; line-height: 1.7; }

.lanes { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; margin-bottom: 22px; }
@media (max-width: 720px) { .lanes { grid-template-columns: 1fr; } }
.lane {
  position: relative; text-align: left; cursor: pointer;
  display: flex; flex-direction: column; gap: 7px;
  padding: 22px 20px 18px; border-radius: 18px;
  border: 1px solid var(--border-soft);
  background: color-mix(in srgb, var(--panel) 50%, transparent);
  transition: transform 0.16s cubic-bezier(0.2, 0.7, 0.2, 1), border-color 0.16s, box-shadow 0.16s, background 0.16s;
}
.lane:hover { transform: translateY(-5px); }
.lane.f {
  background: linear-gradient(165deg, color-mix(in srgb, var(--fast) 14%, transparent), color-mix(in srgb, var(--panel) 60%, transparent) 60%);
  border-color: color-mix(in srgb, var(--fast) 36%, transparent);
}
.lane.f:hover { border-color: var(--fast); box-shadow: 0 16px 40px color-mix(in srgb, var(--fast) 22%, transparent); }
.lane.d {
  background: linear-gradient(165deg, color-mix(in srgb, var(--deep) 14%, transparent), color-mix(in srgb, var(--panel) 60%, transparent) 60%);
  border-color: color-mix(in srgb, var(--deep) 34%, transparent);
}
.lane.d:hover { border-color: var(--deep); box-shadow: 0 16px 40px color-mix(in srgb, var(--deep) 22%, transparent); }
.lane-reco {
  position: absolute; top: 14px; right: 14px;
  font-size: 10.5px; font-weight: 700; letter-spacing: 0.5px;
  padding: 3px 9px; border-radius: 99px; color: #06281b;
  background: linear-gradient(120deg, var(--fast-2), var(--fast));
}
.lane-ic {
  width: 50px; height: 50px; border-radius: 14px; margin-bottom: 6px;
  display: flex; align-items: center; justify-content: center;
}
.lane.f .lane-ic { color: var(--fast-2); background: color-mix(in srgb, var(--fast) 16%, transparent); }
.lane.d .lane-ic { color: var(--deep-2); background: color-mix(in srgb, var(--deep) 16%, transparent); }
.lane-t { font-size: 17px; font-weight: 700; color: var(--ink); }
.lane-s { font-size: 12.5px; color: var(--muted); margin-bottom: 6px; }
.lane-pts { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 7px; }
.lane-pts li { display: flex; align-items: center; gap: 8px; font-size: 12.5px; color: var(--text-2); }
.lane.f .lane-pts li :deep(svg) { color: var(--fast); flex: none; }
.lane.d .lane-pts li :deep(svg) { color: var(--deep-2); flex: none; }
.lane-go {
  display: inline-flex; align-items: center; gap: 4px; margin-top: 14px;
  font-size: 13px; font-weight: 650;
}
.lane.f .lane-go { color: var(--fast); }
.lane.d .lane-go { color: var(--deep-2); }
.lane:hover .lane-go { gap: 8px; }

.intro-foot { display: flex; flex-direction: column; align-items: center; gap: 12px; }
.intro-ent {
  display: inline-flex; align-items: center; gap: 6px;
  border: 1px solid var(--border-soft); border-radius: 99px;
  background: transparent; color: var(--text-2);
  font-size: 12px; padding: 6px 14px; cursor: pointer; transition: border-color 0.14s, color 0.14s;
}
.intro-ent:hover { border-color: var(--border-strong); color: var(--text); }
.intro-ent :deep(svg) { color: var(--muted); }

/* 按钮 */
.wiz-go {
  display: inline-flex; align-items: center; gap: 7px;
  height: 38px; padding: 0 20px;
  border: 1px solid var(--primary);
  background: var(--primary); color: #fff;
  border-radius: 10px; font-size: 13.5px; cursor: pointer;
  transition: filter 0.16s, opacity 0.16s, box-shadow 0.16s;
}
.wiz-go:hover:not(:disabled) { filter: brightness(1.08); }
.wiz-go:disabled { opacity: 0.5; cursor: default; }
.wiz-go.deep { background: var(--deep); border-color: var(--deep); box-shadow: 0 8px 24px color-mix(in srgb, var(--deep) 30%, transparent); }
.wiz-go.ghost { background: transparent; color: var(--text); border-color: var(--border-strong); }
.wiz-go.bg { height: 32px; font-size: 12.5px; color: var(--text-2); }
.wiz-go.bg:hover { color: var(--text); }
.wiz-skip {
  display: block; border: none; background: transparent;
  color: var(--muted); font-size: 12.5px; cursor: pointer; transition: color 0.16s;
}
.wiz-skip:hover { color: var(--text-2); text-decoration: underline; }
.mini {
  display: inline-flex; align-items: center; gap: 5px;
  height: 28px; padding: 0 11px;
  border: 1px solid var(--border-soft);
  background: color-mix(in srgb, var(--panel) 60%, transparent);
  color: var(--text-2); border-radius: 8px; font-size: 12px; cursor: pointer;
}
.mini:hover:not(:disabled) { color: var(--text); border-color: var(--border-strong); }
.mini:disabled { opacity: 0.5; cursor: default; }
.mini.key { color: var(--primary); border-color: color-mix(in srgb, var(--primary) 40%, transparent); }

/* 盘点范围 */
.scope-quick { display: flex; flex-direction: column; gap: 12px; }
.sq-card {
  display: flex; align-items: center; gap: 14px;
  padding: 18px 18px; border-radius: 15px;
  border: 1px solid color-mix(in srgb, var(--fast) 28%, var(--border-soft));
  background: linear-gradient(120deg, color-mix(in srgb, var(--fast) 9%, transparent), color-mix(in srgb, var(--panel) 55%, transparent));
}
.sq-ic {
  width: 46px; height: 46px; border-radius: 12px; flex: none;
  display: flex; align-items: center; justify-content: center;
  color: var(--fast-2); background: color-mix(in srgb, var(--fast) 15%, transparent);
}
.sq-txt { display: flex; flex-direction: column; gap: 4px; flex: 1; min-width: 0; }
.sq-txt b { font-size: 14px; color: var(--ink); }
.sq-txt span { font-size: 12px; color: var(--muted); line-height: 1.5; }
.sq-size { flex: none; font-size: 15px; font-weight: 700; color: var(--fast); font-variant-numeric: tabular-nums; }
.scope-tweak {
  align-self: flex-start; display: inline-flex; align-items: center; gap: 6px;
  border: none; background: transparent; color: var(--text-2); font-size: 12px; cursor: pointer; padding: 2px 0;
}
.scope-tweak:hover { color: var(--text); text-decoration: underline; }
.root-tools { display: flex; align-items: center; gap: 8px; margin-bottom: 10px; flex-wrap: wrap; }
.kw {
  flex: 1; min-width: 200px; height: 30px; padding: 0 11px;
  border: 1px solid var(--border-soft); border-radius: 8px;
  background: color-mix(in srgb, var(--panel) 55%, transparent);
  color: var(--text); font-size: 12.5px;
}
.kw:focus { outline: none; border-color: var(--primary); }
.root-list { display: flex; flex-direction: column; gap: 4px; max-height: 300px; overflow-y: auto; }
.root-row {
  display: flex; align-items: center; gap: 9px;
  padding: 8px 11px; border-radius: 9px;
  border: 1px solid var(--border-soft);
  background: color-mix(in srgb, var(--panel) 45%, transparent);
  cursor: pointer; text-align: left; width: 100%;
  transition: border-color 0.14s, opacity 0.14s;
}
.root-row:hover { border-color: var(--border-strong); }
.root-row.off { opacity: 0.42; }
.root-check {
  display: inline-flex; align-items: center; justify-content: center;
  width: 18px; height: 18px; border-radius: 5px; flex: none;
  border: 1.5px solid var(--border-strong); color: #fff;
}
.root-check.on { background: var(--primary); border-color: var(--primary); }
.root-ic { color: var(--muted); flex: none; }
.root-name { font-size: 13px; color: var(--text); flex: none; }
.root-path { flex: 1; min-width: 0; font-size: 11px; color: var(--dim); font-family: ui-monospace, Consolas, monospace; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.root-bar { flex: none; width: 64px; height: 6px; border-radius: 99px; overflow: hidden; background: color-mix(in srgb, var(--ink) 10%, transparent); }
.root-bar-fill { display: block; height: 100%; border-radius: 99px; background: color-mix(in srgb, var(--primary) 70%, var(--ink) 8%); }
.root-size { flex: none; min-width: 56px; text-align: right; font-size: 11.5px; font-variant-numeric: tabular-nums; color: var(--text-2); }
.root-calc { flex: none; display: inline-flex; align-items: center; gap: 4px; font-size: 11px; color: var(--dim); }

.wiz-foot { display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-top: 20px; }
.wiz-foot.end { justify-content: flex-end; }
.foot-count { font-size: 12.5px; color: var(--muted); }
.foot-count b { color: var(--text); }

/* ───────── 扫描:重点边扫边冒 ───────── */
.scan-live { display: flex; flex-direction: column; gap: 14px; }
.scan-head { display: flex; align-items: center; gap: 16px; }
.scan-center { display: flex; flex-direction: column; align-items: center; justify-content: center; text-align: center; gap: 12px; min-height: 320px; }
.scan-orb { position: relative; width: 56px; height: 56px; flex: none; display: flex; align-items: center; justify-content: center; color: var(--fast-2); }
.scan-orb.stopped { width: 84px; height: 84px; color: var(--muted); border: 2px solid var(--border-strong); border-radius: 50%; }
.scan-ring { position: absolute; inset: 0; border: 2px solid color-mix(in srgb, var(--fast) 45%, transparent); border-top-color: transparent; border-radius: 50%; animation: spin 1s linear infinite; }
.scan-meta { display: flex; flex-direction: column; gap: 2px; }
.scan-num { font-size: 34px; font-weight: 720; font-variant-numeric: tabular-nums; color: var(--ink); line-height: 1.05; }
.scan-num-u { font-size: 13px; font-weight: 500; color: var(--muted); margin-left: 7px; }
.scan-lab { font-size: 13px; color: var(--muted); }
.scan-lab.big { font-size: 14px; color: var(--text); }
.scan-narr { font-size: 13px; color: var(--muted); min-height: 18px; }
.narr-enter-active, .narr-leave-active { transition: opacity 0.4s, transform 0.4s; }
.narr-enter-from { opacity: 0; transform: translateY(6px); }
.narr-leave-to { opacity: 0; transform: translateY(-6px); }
.scan-fine { font-size: 12px; color: var(--dim); line-height: 1.5; }
.scan-actions { display: flex; align-items: center; gap: 10px; margin-top: 6px; }
.scan-actions .flip { transform: rotate(180deg); }

.hot-title { display: flex; align-items: center; gap: 7px; font-size: 12.5px; color: var(--text-2); }
.hot-title :deep(svg) { color: var(--fast-2); }
.hot-live {
  font-size: 10px; font-weight: 700; letter-spacing: 0.5px; color: var(--fast);
  border: 1px solid color-mix(in srgb, var(--fast) 40%, transparent); border-radius: 99px; padding: 1px 7px;
  animation: hotpulse 1.6s ease-in-out infinite;
}
@keyframes hotpulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.45; } }
.hot-list { display: flex; flex-direction: column; gap: 6px; min-height: 280px; max-height: 42vh; overflow-y: auto; padding-right: 4px; }
.hot-row {
  display: flex; align-items: center; gap: 11px;
  padding: 9px 13px; border-radius: 11px;
  border: 1px solid var(--border-soft);
  background: color-mix(in srgb, var(--panel) 48%, transparent);
}
.hot-ic {
  width: 30px; height: 30px; border-radius: 8px; flex: none;
  display: flex; align-items: center; justify-content: center;
  color: var(--text-2); background: color-mix(in srgb, var(--ink) 7%, transparent);
}
.hot-ic.k-image, .hot-ic.k-video { color: var(--deep-2); background: color-mix(in srgb, var(--deep) 12%, transparent); }
.hot-ic.k-doc, .hot-ic.k-text, .hot-ic.k-table { color: var(--fast-2); background: color-mix(in srgb, var(--fast) 12%, transparent); }
.hot-name { flex: 1; min-width: 0; font-size: 13px; color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.hot-meta { flex: none; font-size: 11px; color: var(--dim); font-variant-numeric: tabular-nums; }
.hot-empty { display: flex; align-items: center; gap: 8px; font-size: 12.5px; color: var(--dim); padding: 14px 4px; }
.scan-foot { display: flex; flex-direction: column; align-items: flex-start; gap: 6px; margin-top: 16px; }
/* 重点入场:从右下滑入 + 渐显,营造「往上冒」 */
.hot-enter-active { transition: all 0.45s cubic-bezier(0.2, 0.7, 0.2, 1); }
.hot-leave-active { transition: all 0.25s ease; position: absolute; }
.hot-enter-from { opacity: 0; transform: translateX(18px); }
.hot-leave-to { opacity: 0; transform: translateX(-12px); }
.hot-move { transition: transform 0.4s cubic-bezier(0.2, 0.7, 0.2, 1); }

/* 模型卡(企业) */
.sense-box { margin-top: 16px; padding: 12px 14px; border-radius: 12px; background: color-mix(in srgb, var(--panel) 50%, transparent); border: 1px solid var(--border-soft); }
.sense-head { display: flex; align-items: center; gap: 8px; font-size: 13px; color: var(--text); margin-bottom: 10px; }
.sense-head :deep(svg) { color: var(--primary); flex: none; }
.sense-more { border: none; background: transparent; color: var(--primary); font-size: 11.5px; cursor: pointer; padding: 0; }
.sense-more:hover { text-decoration: underline; }
.sense-warn { margin-left: auto; font-size: 11.5px; color: #e0736b; display: inline-flex; align-items: center; gap: 4px; }
.sense-ok { margin-left: auto; font-size: 11.5px; color: #6fcf97; display: inline-flex; align-items: center; gap: 4px; }
.sense-row { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; padding: 7px 0; border-top: 1px solid var(--border-soft); }
.sr-name { display: inline-flex; align-items: center; gap: 7px; font-size: 12.5px; color: var(--text); min-width: 150px; }
.sr-name em { font-style: normal; font-size: 10px; color: var(--primary); border: 1px solid color-mix(in srgb, var(--primary) 40%, transparent); border-radius: 99px; padding: 0 6px; margin-left: 4px; }
.sr-dot { width: 8px; height: 8px; border-radius: 50%; background: var(--border-strong); flex: none; }
.sr-dot.on { background: #6fcf97; box-shadow: 0 0 6px #6fcf97; }
.sr-key { flex: 1; min-width: 200px; height: 30px; padding: 0 11px; border: 1px solid var(--border-soft); border-radius: 8px; background: color-mix(in srgb, var(--panel) 60%, transparent); color: var(--text); font-size: 12.5px; }
.sr-key:focus { outline: none; border-color: var(--primary); }
.sr-ready { display: inline-flex; align-items: center; gap: 4px; font-size: 12px; color: #6fcf97; }
.sr-msg { font-size: 11.5px; color: var(--muted); }
.sr-prog { flex: 1; min-width: 120px; height: 6px; background: color-mix(in srgb, var(--panel) 80%, transparent); border-radius: 99px; overflow: hidden; }
.sr-fill { display: block; height: 100%; background: var(--primary); border-radius: 99px; transition: width 0.3s; }

/* 图谱 + 迷你检索 */
.graph-wrap { position: relative; height: min(52vh, 480px); border-radius: 14px; overflow: hidden; border: 1px solid var(--border-soft); }
.graph-wrap :deep(.graph) { height: 100%; }
/* 星图入场:整块从中心轻微放大+渐显,配合 fcose 自然铺开,营造「炸开」的仪式感 */
.reveal-pop { animation: revealPop 0.6s cubic-bezier(0.2, 0.7, 0.2, 1); }
@keyframes revealPop { from { opacity: 0; transform: scale(0.97); } to { opacity: 1; transform: none; } }
/* 哇时刻 1 · 字幕仪式 */
.cer-overlay {
  position: absolute; inset: 0; z-index: 4; pointer-events: none;
  display: flex; align-items: center; justify-content: center;
  background: radial-gradient(60% 60% at 50% 45%, color-mix(in srgb, var(--bg) 45%, transparent), transparent 75%);
}
.cer-card { text-align: center; padding: 0 24px; }
.cer-h {
  font-family: var(--serif); font-size: clamp(22px, 3vw, 32px); font-weight: 800; letter-spacing: 1px;
  background: linear-gradient(110deg, var(--fast-2), #5fd0e0 55%, var(--deep-2));
  -webkit-background-clip: text; background-clip: text; color: transparent;
  animation: cerUp 0.6s cubic-bezier(0.2, 0.7, 0.2, 1);
}
.cer-sub { margin-top: 12px; font-size: 15px; color: var(--text); }
.cer-sub b { font-size: 20px; color: var(--fast-2); font-variant-numeric: tabular-nums; }
.cer-themes { display: flex; gap: 8px; justify-content: center; flex-wrap: wrap; margin-top: 12px; }
.cer-chip {
  font-size: 13px; padding: 5px 13px; border-radius: 99px; color: var(--ink);
  border: 1px solid color-mix(in srgb, var(--deep) 32%, var(--border-soft));
  background: color-mix(in srgb, var(--panel) 72%, transparent);
  -webkit-backdrop-filter: blur(8px); backdrop-filter: blur(8px);
  opacity: 0; animation: cerChip 0.5s cubic-bezier(0.2, 0.7, 0.2, 1) forwards;
}
@keyframes cerUp { from { opacity: 0; transform: translateY(12px); } to { opacity: 1; transform: none; } }
@keyframes cerChip { from { opacity: 0; transform: translateY(8px) scale(0.92); } to { opacity: 1; transform: none; } }
.cer-enter-active { transition: opacity 0.4s; }
.cer-leave-active { transition: opacity 0.7s; }
.cer-enter-from, .cer-leave-to { opacity: 0; }
.cer-sub-enter-active { transition: opacity 0.5s, transform 0.5s; }
.cer-sub-enter-from { opacity: 0; transform: translateY(8px); }
/* 收尾 */
.deep-hook {
  display: flex; align-items: center; gap: 14px;
  padding: 16px 18px; margin-bottom: 18px; border-radius: 15px;
  border: 1px solid color-mix(in srgb, var(--deep) 32%, var(--border-soft));
  background: linear-gradient(120deg, color-mix(in srgb, var(--deep) 12%, transparent), color-mix(in srgb, var(--panel) 55%, transparent));
}
.deep-hook.running { border-color: color-mix(in srgb, var(--deep) 40%, transparent); }
.deep-hook.done { border-color: color-mix(in srgb, var(--fast) 38%, var(--border-soft)); background: linear-gradient(120deg, color-mix(in srgb, var(--fast) 11%, transparent), color-mix(in srgb, var(--panel) 55%, transparent)); }
.dh-ic {
  width: 44px; height: 44px; border-radius: 12px; flex: none;
  display: flex; align-items: center; justify-content: center;
  color: var(--deep-2); background: color-mix(in srgb, var(--deep) 15%, transparent);
}
.deep-hook.done .dh-ic { color: var(--fast-2); background: color-mix(in srgb, var(--fast) 15%, transparent); }
.dh-body { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px; }
.dh-t { display: inline-flex; align-items: center; gap: 6px; font-size: 14px; color: var(--ink); }
.dh-d { font-size: 12px; color: var(--muted); line-height: 1.55; }
.tier-quick { color: var(--fast); font-weight: 700; }
.tier-deep { color: var(--deep-2); font-weight: 700; }
.dh-key { display: flex; align-items: center; gap: 7px; margin-top: 9px; flex-wrap: wrap; }
.dh-key input {
  flex: 1; min-width: 190px; height: 30px; padding: 0 11px;
  border: 1px solid color-mix(in srgb, var(--deep) 30%, var(--border-soft)); border-radius: 8px;
  background: color-mix(in srgb, var(--panel) 60%, transparent); color: var(--text); font-size: 12.5px;
}
.dh-key input:focus { outline: none; border-color: var(--deep); }
.dh-key .mini.key { color: var(--deep-2); border-color: color-mix(in srgb, var(--deep) 45%, transparent); }
.dh-msg { font-size: 11.5px; color: var(--fast); margin-top: 6px; }
.dh-btn {
  flex: none; display: inline-flex; align-items: center; gap: 6px;
  height: 34px; padding: 0 15px; border-radius: 10px; cursor: pointer;
  border: 1px solid var(--deep); color: #fff; font-size: 13px;
  background: var(--deep); box-shadow: 0 6px 18px color-mix(in srgb, var(--deep) 28%, transparent);
  transition: filter 0.16s;
}
.dh-btn:hover { filter: brightness(1.08); }

/* ⭐ 专属任务哇时刻:个性化开场白 + 据真实文件的 why 依据 + 错峰浮现 */
.suggest-headline {
  display: flex; align-items: flex-start; gap: 8px;
  font-size: 14.5px; line-height: 1.6; color: var(--ink); margin: 2px 0 14px;
}
.suggest-headline .sh-ic { color: var(--gold, #e9c46a); flex: none; margin-top: 3px; }
.flow-cards { position: relative; display: flex; flex-direction: column; gap: 9px; }
.flow-card {
  display: flex; align-items: center; justify-content: space-between; gap: 12px;
  padding: 14px 16px; border-radius: 13px; cursor: pointer; width: 100%; text-align: left;
  border: 1px solid var(--border-soft);
  background: linear-gradient(120deg, color-mix(in srgb, var(--gold, #e9c46a) 5%, transparent), color-mix(in srgb, var(--panel) 52%, transparent));
  transition: border-color 0.16s, background 0.16s, transform 0.16s, box-shadow 0.16s;
}
.flow-card:hover {
  border-color: color-mix(in srgb, var(--gold, #e9c46a) 55%, transparent);
  background: color-mix(in srgb, var(--gold, #e9c46a) 8%, transparent);
  transform: translateY(-2px);
  box-shadow: 0 10px 26px color-mix(in srgb, var(--gold, #e9c46a) 16%, transparent);
}
.fc-main { display: flex; flex-direction: column; gap: 5px; min-width: 0; }
.fc-t { font-size: 14.5px; color: var(--ink); font-weight: 640; }
.fc-why {
  display: inline-flex; align-items: center; gap: 5px;
  font-size: 11.5px; color: var(--gold, #e9c46a);
  align-self: flex-start; padding: 2px 9px 2px 7px; border-radius: 99px;
  background: color-mix(in srgb, var(--gold, #e9c46a) 12%, transparent);
  border: 1px solid color-mix(in srgb, var(--gold, #e9c46a) 26%, transparent);
}
.fc-why :deep(svg) { flex: none; }
.fc-do { flex: none; display: inline-flex; align-items: center; gap: 3px; font-size: 12.5px; font-weight: 600; color: var(--gold, #e9c46a); }
.flow-card:hover .fc-do { gap: 6px; }
/* 错峰浮现:逐张从下方滑入;LLM 结果替换兜底卡时,旧卡淡出不突兀 */
.flow-enter-active { transition: opacity 0.4s cubic-bezier(0.2, 0.7, 0.2, 1), transform 0.4s cubic-bezier(0.2, 0.7, 0.2, 1); }
.flow-enter-from { opacity: 0; transform: translateY(12px); }
.flow-leave-active { transition: opacity 0.25s; position: absolute; width: 100%; }
.flow-leave-to { opacity: 0; }
.suggest-busy { display: inline-flex; align-items: center; gap: 6px; margin-top: 10px; font-size: 12px; color: var(--muted); }
.suggest-redo { display: inline-flex; align-items: center; gap: 5px; margin-top: 10px; border: none; background: transparent; color: var(--primary); font-size: 12px; cursor: pointer; padding: 2px 0; }
.suggest-redo:hover { text-decoration: underline; }
/* 哇时刻 3 · 明早晨报长期钩子卡 */
.briefing-hook {
  display: flex; align-items: center; gap: 13px; margin-top: 16px;
  padding: 14px 16px; border-radius: 14px;
  border: 1px solid color-mix(in srgb, var(--gold, #e9c46a) 30%, var(--border-soft));
  background: linear-gradient(120deg, color-mix(in srgb, var(--gold, #e9c46a) 10%, transparent), color-mix(in srgb, var(--panel) 55%, transparent));
}
.bh-ic {
  width: 40px; height: 40px; border-radius: 11px; flex: none;
  display: flex; align-items: center; justify-content: center;
  color: var(--gold, #e9c46a); background: color-mix(in srgb, var(--gold, #e9c46a) 15%, transparent);
}
.bh-body { display: flex; flex-direction: column; gap: 3px; min-width: 0; }
.bh-t { font-size: 13.5px; color: var(--ink); }
.bh-d { font-size: 12px; color: var(--muted); line-height: 1.55; }
.finish-row { display: flex; align-items: center; gap: 12px; margin-top: 16px; }
.fine-busy, .fine-ok { display: inline-flex; align-items: center; gap: 5px; font-size: 12px; color: var(--muted); }
.fine-ok { color: var(--primary); }

/* 画像选择(个人 / 企业) */
.prof-cards { display: grid; grid-template-columns: 1fr 1fr; gap: 14px; margin-bottom: 16px; }
.prof-card {
  display: flex; flex-direction: column; gap: 7px; text-align: left;
  padding: 20px 18px; border-radius: 15px; cursor: pointer;
  border: 1px solid var(--border-soft); background: color-mix(in srgb, var(--panel) 45%, transparent);
  color: var(--text-2); transition: border-color 0.14s, background 0.14s, transform 0.14s;
}
.prof-card:hover { border-color: var(--border-strong); transform: translateY(-2px); }
.prof-card.on { border-color: var(--primary); background: color-mix(in srgb, var(--primary) 9%, transparent); }
.pc-ic { width: 44px; height: 44px; border-radius: 12px; display: flex; align-items: center; justify-content: center; color: var(--primary); background: color-mix(in srgb, var(--primary) 13%, transparent); margin-bottom: 4px; }
.pc-t { font-size: 16px; font-weight: 650; color: var(--ink); }
.pc-d { font-size: 12.5px; color: var(--muted); line-height: 1.6; }
.pc-tag { font-size: 11px; color: var(--primary); margin-top: 2px; }
.schema-box { margin-top: 4px; padding: 14px 16px; border-radius: 13px; background: color-mix(in srgb, var(--panel) 50%, transparent); border: 1px solid var(--border-soft); }
.schema-head { display: flex; align-items: center; gap: 8px; font-size: 13px; color: var(--text); margin-bottom: 12px; }
.schema-head :deep(svg) { color: var(--primary); flex: none; }
.schema-fine { margin-left: auto; font-size: 11px; color: var(--dim); }
.schema-grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 10px; }
.schema-card {
  display: flex; flex-direction: column; gap: 4px; text-align: left;
  padding: 13px 14px; border-radius: 12px; cursor: pointer;
  border: 1px solid var(--border-soft); background: color-mix(in srgb, var(--panel) 55%, transparent);
  color: var(--text-2); transition: border-color 0.14s, background 0.14s;
}
.schema-card:hover { border-color: var(--border-strong); }
.schema-card.on { border-color: var(--primary); background: color-mix(in srgb, var(--primary) 9%, transparent); }
.sc-name { font-size: 14px; font-weight: 640; color: var(--ink); }
.sc-ind { font-size: 11px; color: var(--primary); }
.sc-types { font-size: 11.5px; color: var(--muted); line-height: 1.5; }
.sc-src { font-size: 10.5px; color: var(--dim); font-family: ui-monospace, Consolas, monospace; }
.schema-preview { display: flex; align-items: center; gap: 8px; margin-top: 12px; padding: 10px 12px; border-radius: 10px; font-size: 12px; color: var(--text-2); background: color-mix(in srgb, var(--primary) 6%, var(--panel)); border: 1px solid color-mix(in srgb, var(--primary) 20%, transparent); }
.schema-preview :deep(svg) { color: var(--primary); flex: none; }
.schema-preview b { color: var(--text); }
@media (max-width: 760px) {
  .prof-cards { grid-template-columns: 1fr; }
  .schema-grid { grid-template-columns: 1fr 1fr; }
}

.spin { animation: spin 0.9s linear infinite; }
@keyframes spin { to { transform: rotate(360deg); } }
.wiz-fade-enter-active, .wiz-fade-leave-active { transition: opacity 0.2s; }
.wiz-fade-enter-from, .wiz-fade-leave-to { opacity: 0; }
</style>

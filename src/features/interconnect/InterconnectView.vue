<script setup lang="ts">
/**
 * 互联 · 设备联盟 —— 与「协作(项目/任务)」彻底分开的独立板块。
 *
 * 升级方案 v3「互联层重塑」的前端落地:把散乱的连法收进三个 Tab,苹果玻璃琉璃质感。
 *   ① 教程   —— 接入三步 + 连接码 + 局域网直连 + 账号根口令(PRD 原型①)
 *   ② 设备与授权 —— 设备联盟卡(传输徽标 / 挂盘 / 派任务 / 吊销)(PRD 原型②④)
 *   ③ 网络拓扑 —— 本机为心、设备为星的关系图,连接路径只作可视化,不需用户选
 *
 * 传输隐形:用户只面对「连谁 / 用它的什么」,局域网/P2P/中继由系统自动选档,
 * 界面上只作徽标展示。点「派任务」= 先建立远程连接(自动选路)再下发。
 */
import { computed, onMounted, onUnmounted, reactive, ref, watch } from "vue";
import {
  MonitorSmartphone,
  Server,
  Copy,
  RefreshCw,
  LoaderCircle,
  ShieldOff,
  Radio,
  Laptop,
  Smartphone,
  HardDrive,
  GraduationCap,
  Network,
  Send,
  Wifi,
  Zap,
  Globe,
  FolderInput,
  ShieldCheck,
  Cpu,
} from "@lucide/vue";
import { invoke, isTauri } from "../../tauri";
import { useCollabStore } from "../collab/stores/collab";
import { useAppStore } from "../../stores/app";
import { collabApi, type AdminDevice, type AuditRow } from "../collab/api";
import { toast } from "../../composables/useToast";
import {
  loadRemoteSources,
  upsertRemoteSource,
  removeRemoteSource,
  type RemoteSource,
} from "./remoteSources";

const collab = useCollabStore();
const app = useAppStore();

const owner = computed(() => (collab.user?.role ?? "") === "owner");
const authed = computed(() => collab.authed);
const needsBootstrap = computed(() => !!collab.hostInfo?.needsBootstrap);
// 桌面(Tauri):本机是否已「设为主机」由 collab_host_status 决定。
// 浏览器 / Docker server 版:你本就在跟一台主机(server)对话,视作「已是主机」,
// 直接亮连接信息,而不是让用户去点「设为主机」。
const hostRunning = computed(() =>
  isTauri ? !!collab.hostInfo?.running : true
);
// 三态:①未设为主机(仅桌面) ②已是主机但未认证(仅桌面,需注册/登录管理者) ③可出连接码
const showBecomeHost = computed(() => isTauri && !hostRunning.value);
const showHostAuth = computed(() => isTauri && hostRunning.value && !authed.value);
const showConnect = computed(() => hostRunning.value && (authed.value || !isTauri));

// ── 顶部 Tab ──
type Tab = "guide" | "devices" | "topo";
const tab = ref<Tab>("guide");
const TABS: { key: Tab; label: string; icon: unknown }[] = [
  { key: "guide", label: "远程连接教程", icon: GraduationCap },
  { key: "devices", label: "设备与授权", icon: MonitorSmartphone },
  { key: "topo", label: "网络拓扑", icon: Network },
];

// ── 桌面:设为主机后注册/登录管理者(互联板块自足,不用去协作) ──
const authForm = reactive({ username: "", password: "", displayName: "" });
const authBusy = ref(false);
const authErr = ref("");
async function doHostAuth() {
  authErr.value = "";
  const u = authForm.username.trim();
  if (!u || !authForm.password) {
    authErr.value = "请填写用户名和密码";
    return;
  }
  authBusy.value = true;
  try {
    if (needsBootstrap.value) {
      await collab.bootstrap(u, authForm.password, authForm.displayName.trim() || u, true);
    } else {
      await collab.login(u, authForm.password);
    }
    authForm.password = "";
    await refreshAll();
  } catch (e) {
    authErr.value = (e as Error).message;
  } finally {
    authBusy.value = false;
  }
}

// ── 主机连接:owner 令牌(手机以完整权限连上本机的凭据) ──
const tokenRevealed = ref(false);
const tokenCopied = ref(false);
const maskedToken = computed(() => {
  const t = collab.token || "";
  return t
    ? t.slice(0, 4) + "•".repeat(Math.max(4, t.length - 8)) + t.slice(-4)
    : "";
});
async function copyToken() {
  const t = collab.token;
  if (!t) return;
  try {
    await navigator.clipboard.writeText(t);
    tokenCopied.value = true;
    setTimeout(() => (tokenCopied.value = false), 1500);
    toast.info("owner 令牌已复制");
  } catch {
    toast.error("复制失败,请手动选中");
  }
}

// PLRK1 连接码:把「本机地址 + owner 令牌」打包成一串(base64url),同账号自己的设备
// 粘这一串即以 owner 完整权限连上,不用分别填地址和令牌。仅供你自己的设备用。
const connectCode = computed(() => {
  const t = collab.token || "";
  const a = collab.hostInfo?.urls ?? [];
  const n = collab.hostInfo?.nodeId ?? ""; // iroh 主机 NodeId → 手机打洞 P2P 直连
  if (!t) return "";
  try {
    const payload: Record<string, unknown> = { t, a };
    if (n) payload.n = n;
    const b64 = btoa(JSON.stringify(payload))
      .replace(/\+/g, "-")
      .replace(/\//g, "_")
      .replace(/=+$/, "");
    return "PLRK1-" + b64;
  } catch {
    return "";
  }
});
const hasIroh = computed(() => !!collab.hostInfo?.nodeId);
const codeCopied = ref(false);
const showManual = ref(false);
async function copyConnectCode() {
  if (!connectCode.value) return;
  try {
    await navigator.clipboard.writeText(connectCode.value);
    codeCopied.value = true;
    setTimeout(() => (codeCopied.value = false), 1500);
    toast.info("连接码已复制,去手机 App 粘贴即可");
  } catch {
    toast.error("复制失败,请手动选中");
  }
}

// ── 局域网直连开关(手机走 WiFi 连的最后一环) ──
const remoteOn = computed(() => !!collab.hostInfo?.remoteAccess);
const lanBusy = ref(false);
async function toggleRemote() {
  if (!isTauri || lanBusy.value) return;
  const target = !remoteOn.value;
  lanBusy.value = true;
  try {
    await collab.hostSetRemoteAccess(target); // 后端重启主机重绑,urls 随之变
    await refreshAll();
    toast.info(target ? "已开启局域网直连:连接码已带上局域网 IP,同一 WiFi 的手机可连" : "已关闭:主机回到仅本机可连");
  } catch (e) {
    toast.error((e as Error).message);
  } finally {
    lanBusy.value = false;
  }
}

async function becomeHost() {
  try {
    await collab.hostStart();
    toast.info("本机已设为主机");
    await refreshAll();
  } catch (e) {
    toast.error(`设为主机失败:${(e as Error).message}`);
  }
}

// ── 设备看板 ──
const devices = ref<AdminDevice[]>([]);
const devicesLoading = ref(false);
async function loadDevices(silent = false) {
  if (!silent) devicesLoading.value = true;
  try {
    devices.value = await collabApi.adminDevices();
  } catch {
    if (!silent) devices.value = []; // 静默刷新失败保留旧数据,不闪空
  } finally {
    devicesLoading.value = false;
  }
}
async function revoke(d: AdminDevice) {
  if (!confirm(`吊销设备「${d.name || d.node_id || d.id}」?该设备将无法再连入。`)) return;
  try {
    await collabApi.adminDeviceRevoke(d.id);
    toast.info("已吊销");
    await loadDevices();
  } catch (e) {
    toast.error((e as Error).message);
  }
}
function devIcon(d: AdminDevice) {
  const n = (d.name || "").toLowerCase();
  if (d.is_host) return Server;
  if (/phone|手机|android|ios|mobile|pixel|iphone/.test(n)) return Smartphone;
  if (/nas|群晖|synology|server/.test(n)) return HardDrive;
  return Laptop;
}
function shortNode(id?: string) {
  if (!id) return "—";
  return id.length > 16 ? `${id.slice(0, 8)}…${id.slice(-6)}` : id;
}

// ── 传输选档(可视化):局域网直连 / P2P 打洞 / 中继兜底 ──
// 没有 per-device 心跳前,按本机当前实际连法推断徽标(host 有 iroh NodeId → 走 P2P;
// 仅开局域网 → LAN)。这是「系统自动选了哪一档」的如实展示,不是编造的实时指标。
type Transport = "local" | "lan" | "p2p" | "relay" | "disk";
const TRANSPORT: Record<Transport, { label: string; icon: unknown; cls: string }> = {
  local: { label: "本机", icon: Server, cls: "t-local" },
  lan: { label: "局域网直连", icon: Wifi, cls: "t-lan" },
  p2p: { label: "P2P 打洞", icon: Zap, cls: "t-p2p" },
  relay: { label: "中继兜底", icon: Globe, cls: "t-relay" },
  disk: { label: "远程盘", icon: HardDrive, cls: "t-disk" },
};
function devTransport(d: AdminDevice): Transport {
  if (d.is_host) return "local";
  if (d.revoked) return "relay";
  if (hasIroh.value) return "p2p";
  if (remoteOn.value) return "lan";
  return "relay";
}

const hostDevice = computed(() => devices.value.find((d) => d.is_host) || null);
const remoteDevices = computed(() => devices.value.filter((d) => !d.is_host));
const onlineCount = computed(() => devices.value.filter((d) => !d.revoked).length);

// ── 设备联盟:本机真实遥测(CPU/内存/磁盘)。远端设备的同款数据由各自 Polaris 上报(Phase2)。──
interface DeviceStats {
  cpu_pct: number;
  mem_used: number;
  mem_total: number;
  disk_used: number;
  disk_total: number;
  cores: number;
}
const localStats = ref<DeviceStats | null>(null);
let statTimer: ReturnType<typeof setInterval> | null = null;
async function sampleLocal() {
  if (!isTauri) return;
  try {
    localStats.value = await invoke<DeviceStats>("sys_stats");
  } catch {
    /* 采样失败静默,下次再采 */
  }
}
/** 设备的资源数据:本机=真实采样;远端=遥测最近一帧(手机等自己上报,字段按能力可缺)。 */
function statsFor(d: AdminDevice): Partial<DeviceStats> | null {
  if (d.is_host) return localStats.value;
  return d.stats ?? null;
}
/** 遥测新鲜度:超 90s 没上报视为过期(卡上标灰,不冒充实时)。 */
function statsStale(d: AdminDevice): boolean {
  if (d.is_host) return false;
  return !d.stats_at || Date.now() - d.stats_at > 90_000;
}
function pctOf(used: number, total: number): number {
  return total > 0 ? Math.min(100, Math.round((used / total) * 100)) : 0;
}
/** 字节 → 人类可读:≥1024G 用 T,否则 G(≥10 取整,否则 1 位小数)。 */
function fmtSize(bytes: number): string {
  const g = bytes / 1024 ** 3;
  if (g >= 1024) return (g / 1024).toFixed(1) + "T";
  return (g >= 10 ? Math.round(g).toString() : g.toFixed(1)) + "G";
}
/** 仪表条颜色:占用越高越暖(≥85% 红、≥60% 橙、否则蓝绿)。 */
function meterCls(p: number): string {
  return p >= 85 ? "m-hot" : p >= 60 ? "m-warm" : "m-cool";
}
/** 部分字段的 stats → 仪表行(设备能报什么画什么,缺的不编)。 */
function metersOf(s: Partial<DeviceStats> | null): { k: string; p: number; v: string }[] {
  if (!s) return [];
  const rows: { k: string; p: number; v: string }[] = [];
  if (typeof s.cpu_pct === "number") {
    const c = Math.round(s.cpu_pct);
    rows.push({ k: "CPU", p: c, v: c + "%" });
  }
  if (s.mem_total) {
    const used = s.mem_used ?? 0;
    rows.push({
      k: "内存",
      p: s.mem_used != null ? pctOf(used, s.mem_total) : 0,
      v: s.mem_used != null ? fmtSize(used) + "/" + fmtSize(s.mem_total) : fmtSize(s.mem_total),
    });
  }
  if (s.disk_total) {
    rows.push({
      k: "磁盘",
      p: pctOf(s.disk_used ?? 0, s.disk_total),
      v: fmtSize(s.disk_used ?? 0) + "/" + fmtSize(s.disk_total),
    });
  }
  return rows;
}
/** 设备卡三条仪表;无任何数据(远端未上报)返回 null。 */
function metersFor(d: AdminDevice): { k: string; p: number; v: string }[] | null {
  const rows = metersOf(statsFor(d));
  return rows.length ? rows : null;
}
function coresFor(d: AdminDevice): number | null {
  return statsFor(d)?.cores ?? null;
}

// ── 子导航:我的设备 / 我共享出去的 / 我能用的(出站盘)/ 正在发生 ──
type DevFilter = "mine" | "shared" | "usable" | "activity";
const devFilter = ref<DevFilter>("mine");
const DEV_FILTERS = computed(() => [
  { key: "mine" as DevFilter, label: "我的设备", n: devices.value.length },
  { key: "shared" as DevFilter, label: "我共享出去的", n: 0 },
  { key: "usable" as DevFilter, label: "我能用的", n: remotes.value.length },
  { key: "activity" as DevFilter, label: "正在发生", n: auditRows.value.length },
]);

// ── 远程盘(我能用的)实况:经 iroh 隧道调对端 sys_stats,真数据;对端老版本无此命令→缺项 ──
const remoteStats = ref<Record<string, Partial<DeviceStats>>>({});
let remotePolling = false;
async function pollRemoteStats() {
  if (remotePolling) return; // 单飞:12s tick 与手动可能重入,别叠(codex #4)
  remotePolling = true;
  // 修剪已移除远程盘的残留键,防 map 无限膨胀。
  const liveIds = new Set(remotes.value.map((s) => s.id));
  for (const k of Object.keys(remoteStats.value)) {
    if (!liveIds.has(k)) delete remoteStats.value[k];
  }
  try {
    await Promise.all(
      remotes.value.map(async (s) => {
        const ctl = new AbortController();
        const t = setTimeout(() => ctl.abort(), 6000); // 隧道半死时别挂死(覆盖到 json() 完成)
        try {
          const r = await fetch(`http://127.0.0.1:${s.port}/api/invoke`, {
            method: "POST",
            headers: {
              "content-type": "application/json",
              ...(s.token ? { Authorization: `Bearer ${s.token}` } : {}),
            },
            body: JSON.stringify({ cmd: "sys_stats", args: {} }),
            signal: ctl.signal,
          });
          if (!r.ok) throw new Error(String(r.status));
          const stats = await r.json();
          remoteStats.value = { ...remoteStats.value, [s.id]: stats };
        } catch {
          // 断线/超时:删掉旧实况,别把过期值一直冒充「实况」(codex #4)。
          if (remoteStats.value[s.id]) {
            const next = { ...remoteStats.value };
            delete next[s.id];
            remoteStats.value = next;
          }
        } finally {
          clearTimeout(t);
        }
      })
    );
  } finally {
    remotePolling = false;
  }
}

// ── 正在发生:audit 活动流(接入/上报/吊销/账号事件,主机留痕) ──
const auditRows = ref<AuditRow[]>([]);
const auditLoading = ref(false);
async function loadAudit(silent = false) {
  if (!silent) auditLoading.value = true;
  try {
    auditRows.value = await collabApi.adminAudit(50);
  } catch {
    if (!silent) auditRows.value = []; // 静默刷新失败保留旧数据
  } finally {
    auditLoading.value = false;
  }
}
const AUDIT_LABEL: Record<string, string> = {
  "auth.login": "设备登录接入",
  "user.create": "新账号创建",
  "user.disable": "账号停用",
  "device.telemetry": "设备开始上报资源",
  "device.revoke": "设备被吊销",
  "mirror.export": "账号镜像导出",
};
function auditLabel(a: AuditRow): string {
  return AUDIT_LABEL[a.action] ?? a.action;
}
/** Unix 秒 → 相对时间。 */
function relTime(atSec: number): string {
  const d = Date.now() - atSec * 1000;
  if (d < 60_000) return "刚刚";
  if (d < 3_600_000) return `${Math.floor(d / 60_000)} 分钟前`;
  if (d < 86_400_000) return `${Math.floor(d / 3_600_000)} 小时前`;
  return `${Math.floor(d / 86_400_000)} 天前`;
}

// 进对应子页即取数;设备页周期静默刷新让遥测仪表流动。
watch(devFilter, (f) => {
  if (f === "activity") loadAudit();
  if (f === "usable") pollRemoteStats();
});

// ── 派任务 = 左侧新建一个「标记了目标设备」的对话 ──
// 点派任务:主对话系统里 createConversation → 标题打上 @设备名 → 切到对话页,
// 你在里面下指令,就是发给那台机器的任务(真正跨设备下发执行走 Phase 3.5)。
const dispatchBusy = ref(false);
async function openDispatch(d: AdminDevice) {
  if (dispatchBusy.value) return;
  dispatchBusy.value = true;
  const label = d.name || shortNode(d.node_id);
  try {
    let pid = app.currentProjectId ?? app.projects[0]?.id ?? null;
    if (!pid) {
      const p = await app.createProject("设备任务");
      pid = p.id;
    }
    const conv = await app.createConversation(pid, true); // 切到对话视图
    await app.renameConversation(conv, `@${label} · 派任务`);
    toast.info(`已开对话「@${label}」—— 在这里下指令,就是派给它的任务`);
  } catch (e) {
    toast.error(`开任务对话失败:${(e as Error).message}`);
  } finally {
    dispatchBusy.value = false;
  }
}

// ── 挂盘(远程盘):P2 路线图,先给出连接与说明 ──
function mountDisk(d: AdminDevice) {
  toast.info(`正在把「${d.name || "远程设备"}」的盘挂为本机盘 —— 远程盘(WebDAV)接入中,先用「派任务」验证连接`);
}

// ── 网络拓扑几何:本机为心,远端设备/远程盘环绕成星 ──
// 两类节点:① 入站设备(手机等主动连进本机做主机);② 出站远程盘(本机拨出去连的
// NAS/远程源,走 iroh 隧道,登记在文件中心)。后者原来只在文件中心可见,这里也画进星图。
interface TopoEntity {
  kind: "device" | "disk";
  name: string;
  emoji: string;
  revoked: boolean;
  nodeId: string;
  t: Transport;
}
const topoEntities = computed<TopoEntity[]>(() => {
  const devs: TopoEntity[] = remoteDevices.value.map((d) => ({
    kind: "device",
    name: d.name || shortNode(d.node_id),
    emoji: /phone|手机|android|ios|iphone/i.test(d.name || "")
      ? "📱"
      : /nas|群晖|synology/i.test(d.name || "")
        ? "🗄"
        : "💻",
    revoked: !!d.revoked,
    nodeId: d.node_id || "",
    t: devTransport(d),
  }));
  const disks: TopoEntity[] = remotes.value.map((s) => ({
    kind: "disk",
    name: s.name || "远程盘",
    emoji: "🗄",
    revoked: false,
    nodeId: s.nodeId || "",
    t: "disk",
  }));
  return [...devs, ...disks];
});
const topo = computed(() => {
  const W = 600;
  const H = 380;
  const cx = W / 2;
  const cy = H / 2;
  const R = 132;
  const list = topoEntities.value;
  const n = list.length;
  const nodes = list.map((e, i) => {
    // 从正上方起,均匀铺开;单个时略偏右上更自然
    const deg = n === 1 ? -50 : -90 + (i * 360) / n;
    const rad = (deg * Math.PI) / 180;
    return {
      ...e,
      x: cx + R * Math.cos(rad),
      y: cy + R * Math.sin(rad),
    };
  });
  return { W, H, cx, cy, R, nodes };
});

// ── 接入远程主机 / NAS(iroh P2P):填 NodeId + owner 令牌 → 起隧道 → 登记为远程源 ──
// 隧道在 127.0.0.1:port 起透明代理,「文件中心 · 远程源」经该端口浏览对端的盘。
const remotes = ref<RemoteSource[]>(loadRemoteSources());
const addForm = reactive({ name: "群晖 NAS", nodeId: "", token: "", open: false });
let portSeq = 18620; // 本地代理端口起点,逐个 +1 避冲突
const connBusy = ref(false);

async function connectRemote() {
  const nodeId = addForm.nodeId.trim();
  if (!nodeId) {
    toast.error("请填 NAS 的 iroh NodeId(或连接码)");
    return;
  }
  if (!isTauri) {
    toast.error("远程主机接入需在桌面 App 内(iroh 隧道)");
    return;
  }
  // 端口在已用远程源之上取,避免与既有隧道撞。
  const used = new Set(remotes.value.map((r) => r.port));
  while (used.has(portSeq)) portSeq++;
  const port = portSeq++;
  connBusy.value = true;
  try {
    await invoke("collab_tunnel_connect", { hostNodeId: nodeId, listenPort: port });
    const src: RemoteSource = {
      id: `rs-${Date.now()}`,
      name: addForm.name.trim() || "远程主机",
      nodeId,
      port,
      token: addForm.token.trim(),
      createdAt: Date.now(),
    };
    remotes.value = upsertRemoteSource(src);
    addForm.open = false;
    addForm.nodeId = "";
    addForm.token = "";
    toast.info(`已发起 iroh P2P 连接「${src.name}」—— 到「文件中心 · 远程源」浏览它的盘`);
  } catch (e) {
    toast.error(`连接失败:${(e as Error).message}`);
  } finally {
    connBusy.value = false;
  }
}
function forgetRemote(s: RemoteSource) {
  if (!confirm(`断开并移除「${s.name}」?`)) return;
  remotes.value = removeRemoteSource(s.id);
  toast.info("已移除远程源");
}

async function refreshAll() {
  remotes.value = loadRemoteSources(); // 文件中心新接入的远程盘同步进拓扑
  if (isTauri) await collab.hostStatus();
  await loadDevices();
}

// 「连接码只粘一次」:已保存的远程盘(NAS 等)在启动时自动重建 iroh 隧道,
// 用户无需再粘码。端口沿用保存时的值,失败静默(下次刷新/手动再连)。
async function autoReconnectRemotes() {
  if (!isTauri) return;
  for (const s of loadRemoteSources()) {
    try {
      await invoke("collab_tunnel_connect", { hostNodeId: s.nodeId, listenPort: s.port });
    } catch {
      /* 单台失败不阻断其它;拓扑里仍显示,浏览时按需重连 */
    }
  }
}

onMounted(async () => {
  await refreshAll();
  // 流程顺滑:已经有设备/远程盘的老用户,进来直接看设备看板,不用每次翻过教程。
  if (tab.value === "guide" && (remoteDevices.value.length || remotes.value.length)) {
    tab.value = "devices";
  }
  await autoReconnectRemotes();
  await sampleLocal();
  // 本机仪表每 4s 跳一帧;设备页每 3 拍(12s)静默刷一次远端遥测/远程盘实况。
  let tick = 0;
  statTimer = setInterval(() => {
    sampleLocal();
    if (++tick % 3 === 0 && tab.value === "devices") {
      if (devFilter.value === "mine") loadDevices(true);
      else if (devFilter.value === "usable") pollRemoteStats();
      else if (devFilter.value === "activity") loadAudit(true); // 停在活动页也持续刷新(codex #7)
    }
  }, 4000);
});
onUnmounted(() => {
  if (statTimer) clearInterval(statTimer);
});
</script>

<template>
  <div class="interconnect">
    <header class="bar">
      <div class="ttl"><Radio :size="17" :stroke-width="1.8" /> 互联 · 设备联盟</div>
      <nav class="tabs">
        <button
          v-for="t in TABS"
          :key="t.key"
          class="tab"
          :class="{ on: tab === t.key }"
          @click="tab = t.key"
        >
          <component :is="t.icon" :size="14" :stroke-width="1.9" />
          <span class="tab-label">{{ t.label }}</span>
        </button>
      </nav>
      <button class="icobtn" title="刷新" @click="refreshAll"><RefreshCw :size="15" /></button>
    </header>

    <div class="scroll">
      <!-- ════════════ ① 远程连接教程 ════════════ -->
      <template v-if="tab === 'guide'">
        <!-- 接入三步(传输隐形宣言) -->
        <section class="glass steps-card">
          <div class="sc-head">
            <span class="sc-kick">接入联盟 · 三步</span>
            <h2 class="sc-title">一个口令,设备<span class="grad">自动入网</span></h2>
            <p class="sc-sub">你只选「连谁、用它的什么」。局域网 / P2P 打洞 / 中继兜底由系统自动选,你永不用管。</p>
          </div>
          <ol class="steps">
            <li>
              <span class="st-n">1</span>
              <div><b>这台电脑生成连接码</b><p>下方一串即是。桌面版会自动带上 iroh P2P 直连能力。</p></div>
            </li>
            <li>
              <span class="st-n">2</span>
              <div><b>新设备粘一下</b><p>手机 / 另一台电脑装好 Polaris,把连接码粘进去 —— 不用填地址、不用选连法。</p></div>
            </li>
            <li>
              <span class="st-n">3</span>
              <div><b>传输自动选路</b><p>同一 WiFi 走局域网直连;跨网自动 P2P 打洞;打不通中继兜底。连上就能挂盘、派任务。</p></div>
            </li>
          </ol>
        </section>

        <!-- 主机连接卡:app 该填什么,一次看清 -->
        <section class="glass hero">
          <div class="hero-head">
            <MonitorSmartphone :size="18" :stroke-width="1.8" />
            <span>让手机 / 其它设备连上这台电脑</span>
          </div>

          <template v-if="showConnect">
            <div class="hint">
              <b>手机连这台电脑,就一步</b>:手机 App 里<b>粘下面这串</b>就连上 —— 经 <b>iroh 打洞 P2P 直连</b>(打不通自动走中继兜底),以 owner 完整权限。装完 App 直接粘,不用登录、不用授权。
              <template v-if="!hasIroh"><br/>(本机 iroh 正在就绪,连接码稍后会自动带上 P2P 直连能力,刷新本页即可。)</template>
            </div>

            <div class="code-box" @click="copyConnectCode">
              <span v-if="connectCode" class="code">{{ connectCode }}</span>
              <span v-else class="code dim">未登录管理者 —— 先在下方注册/登录</span>
            </div>
            <div class="code-actions">
              <button class="pill" @click="copyConnectCode" :disabled="!connectCode">
                <Copy :size="13" /> {{ codeCopied ? "已复制 ✓" : "复制连接码" }}
              </button>
              <button class="pill ghost" @click="showManual = !showManual">
                {{ showManual ? "收起" : "手动填地址/令牌" }}
              </button>
            </div>

            <div v-if="showManual" class="manual">
              <div class="field">
                <div class="fl">本机地址</div>
                <div class="addr-list">
                  <template v-if="collab.hostInfo?.urls?.length">
                    <code v-for="u in collab.hostInfo.urls" :key="u">{{ u }}</code>
                  </template>
                  <span v-else class="dim">仅本机回环 —— 手机要连需开局域网(allow_lan)或走中继</span>
                  <span v-if="collab.hostInfo?.port" class="al">端口 {{ collab.hostInfo.port }}</span>
                </div>
              </div>
              <div class="field">
                <div class="fl">owner 令牌</div>
                <div class="code-box sm" @click="tokenRevealed = !tokenRevealed">
                  <span v-if="collab.token" class="code">{{ tokenRevealed ? collab.token : maskedToken }}</span>
                  <span v-else class="code dim">未登录</span>
                </div>
                <div class="code-actions">
                  <button class="pill ghost" @click="copyToken" :disabled="!collab.token">
                    <Copy :size="12" /> {{ tokenCopied ? "已复制 ✓" : "复制令牌" }}
                  </button>
                  <button class="pill ghost" @click="tokenRevealed = !tokenRevealed" :disabled="!collab.token">
                    {{ tokenRevealed ? "隐藏" : "显示" }}
                  </button>
                </div>
              </div>
            </div>

            <div v-if="isTauri" class="lan-toggle" :class="{ busy: lanBusy }" @click="toggleRemote">
              <div class="lt-txt">
                <span class="lt-title">允许手机走 WiFi 连(局域网直连)</span>
                <span class="lt-sub">{{ remoteOn
                  ? "已开 · 连接码含局域网 IP,同一 WiFi 的手机可连"
                  : "关 · 仅本机可连;手机连不上就打开这个" }}</span>
              </div>
              <span class="switch" :class="{ on: remoteOn }"><i></i></span>
            </div>

            <p class="foot-note">
              仅供你<b>自己的设备</b>用。想让<b>别人(不同账号)</b>加入?到「协作」生成邀请码(collaborator/visitor)。
              要手机从外网(不同 WiFi)连,需走中继/隧道。
            </p>
          </template>

          <template v-else-if="showHostAuth">
            <div class="hint">
              {{ needsBootstrap
                ? "本机主机已启动 —— 注册一个管理者账号,就能拿到手机连接码。"
                : "本机主机已启动 —— 登录管理者账号以取连接码。" }}
            </div>
            <div class="auth-form">
              <input v-model="authForm.username" class="af-inp" placeholder="用户名" autocomplete="username" />
              <input v-if="needsBootstrap" v-model="authForm.displayName" class="af-inp" placeholder="昵称(可选)" />
              <input v-model="authForm.password" type="password" class="af-inp" placeholder="密码" autocomplete="current-password" @keydown.enter="doHostAuth" />
              <button class="cta" :disabled="authBusy" @click="doHostAuth">
                <LoaderCircle v-if="authBusy" :size="15" class="spin" />
                {{ needsBootstrap ? "注册管理者" : "登录" }}
              </button>
              <p v-if="authErr" class="af-err">{{ authErr }}</p>
            </div>
          </template>

          <template v-else>
            <div class="hint">这台电脑还不是主机。设为主机后,手机等设备就能连进来用它的算力与文件。</div>
            <button class="cta" @click="becomeHost">
              <Server :size="16" /> 把这台电脑设为主机
            </button>
          </template>
        </section>
      </template>

      <!-- ════════════ ② 设备与授权 ════════════ -->
      <template v-else-if="tab === 'devices'">
        <!-- 联盟概览条 -->
        <section class="glass fed-head">
          <span class="fh-av"></span>
          <div class="fh-txt">
            <div class="fh-title">{{ collab.user?.display_name || collab.user?.username || "我" }} 的联盟</div>
            <div class="fh-sub"><span class="odot"></span> {{ onlineCount }} 台在线 · 传输自动选路,你只管用</div>
          </div>
          <button class="icobtn sm" title="刷新" @click="loadDevices()"><RefreshCw :size="14" /></button>
        </section>

        <!-- 子导航:我的设备 / 我共享出去的 / 我能用的 / 正在发生 -->
        <nav class="dev-subnav">
          <button
            v-for="f in DEV_FILTERS"
            :key="f.key"
            class="dsn"
            :class="{ on: devFilter === f.key }"
            @click="devFilter = f.key"
          >
            {{ f.label }}<span v-if="f.n" class="dsn-n">{{ f.n }}</span>
          </button>
        </nav>

        <!-- 我能用的:出站远程盘(NAS 等),这台电脑拨出去连、用它的盘 -->
        <section v-if="devFilter === 'usable'" class="glass grant-note">
          <div class="gn-head"><Network :size="15" :stroke-width="1.9" /> 接入远程主机 / NAS(iroh P2P)</div>
          <p class="foot-note" style="margin-top:0">
            粘 NAS 的 <b>iroh NodeId</b> 与 <b>owner 令牌</b>,系统经 iroh 打洞直连(打不通走中继),
            连上后到「文件中心 · 远程源」像本机一样浏览、下载它的盘。
          </p>
          <div v-for="s in remotes" :key="s.id" class="remote-block">
            <div class="dev-line remote-line">
              <span class="conn t-p2p"><Zap :size="12" :stroke-width="2" /> {{ s.name }}</span>
              <span class="dev-node">127.0.0.1:{{ s.port }}</span>
              <button class="b danger" title="断开并移除" @click="forgetRemote(s)"><ShieldOff :size="13" /></button>
            </div>
            <!-- 对端实况:经隧道调 sys_stats(真数据;对端老版本没有则待上报) -->
            <div class="dev-meters" v-if="metersOf(remoteStats[s.id] ?? null).length">
              <div class="mt" v-for="m in metersOf(remoteStats[s.id] ?? null)" :key="m.k">
                <span class="mt-k">{{ m.k }}</span>
                <span class="mt-bar"><i :class="meterCls(m.p)" :style="{ width: m.p + '%' }"></i></span>
                <span class="mt-v">{{ m.v }}</span>
              </div>
            </div>
            <div class="dev-meters-none" v-else>
              <Cpu :size="12" :stroke-width="1.9" /> 实况待上报(对端需新版镜像)
            </div>
          </div>
          <div v-if="addForm.open" class="auth-form" style="margin-top:12px; max-width:none">
            <input v-model="addForm.name" class="af-inp" placeholder="名称(如:群晖 NAS)" />
            <input v-model="addForm.nodeId" class="af-inp" placeholder="NAS iroh NodeId / 连接码" />
            <input v-model="addForm.token" class="af-inp" placeholder="owner 令牌(浏览鉴权)" />
            <button class="cta" :disabled="connBusy" @click="connectRemote">
              <LoaderCircle v-if="connBusy" :size="15" class="spin" />
              <Zap v-else :size="15" /> 发起 iroh 连接
            </button>
          </div>
          <button v-else class="pill ghost" style="margin-top:12px; flex:none" @click="addForm.open = true">
            + 接入一台 NAS / 主机
          </button>
        </section>

        <!-- 我的设备:入站设备(含本机),带真实/待上报资源仪表 -->
        <template v-if="devFilter === 'mine'">
        <div v-if="devicesLoading" class="empty glass"><LoaderCircle :size="14" class="spin" /> 加载中…</div>
        <div v-else-if="!devices.length" class="empty glass">
          还没有已登记设备。{{ owner ? "去「教程」把手机用连接码连进来,就会出现在这里。" : "登录主机后可见。" }}
        </div>

        <template v-else>
          <!-- 设备联盟卡片(PRD 原型②) -->
          <div class="dev-grid">
            <article
              v-for="d in devices"
              :key="d.id"
              class="glass dev"
              :class="[TRANSPORT[devTransport(d)].cls, { off: d.revoked }]"
            >
              <div class="dev-top">
                <span class="dev-ico"><component :is="devIcon(d)" :size="19" :stroke-width="1.7" /></span>
                <div class="dev-id">
                  <div class="dev-name">
                    {{ d.name || d.node_id || d.id }}
                    <span v-if="d.is_host" class="host-badge">主机</span>
                  </div>
                  <div class="dev-owner">{{ d.username ? `@${d.username}` : `用户 #${d.user_id}` }}</div>
                </div>
                <span class="dev-dot" :class="{ on: !d.revoked }"></span>
              </div>

              <!-- 传输徽标 + 核数 + 连接身份(如实展示系统选了哪一档,非编造指标) -->
              <div class="dev-line">
                <span class="conn" :class="TRANSPORT[devTransport(d)].cls">
                  <component :is="TRANSPORT[devTransport(d)].icon" :size="12" :stroke-width="2" />
                  {{ TRANSPORT[devTransport(d)].label }}
                </span>
                <span v-if="coresFor(d)" class="dev-cores">{{ coresFor(d) }} 核</span>
                <span class="dev-node">{{ shortNode(d.node_id) }}</span>
              </div>

              <!-- 资源仪表:本机=真实采样(每 4s 跳);远端=遥测上报(设备能报什么画什么) -->
              <div class="dev-meters" :class="{ stale: statsStale(d) }" v-if="metersFor(d)">
                <div class="mt" v-for="m in metersFor(d)!" :key="m.k">
                  <span class="mt-k">{{ m.k }}</span>
                  <span class="mt-bar"><i :class="meterCls(m.p)" :style="{ width: m.p + '%' }"></i></span>
                  <span class="mt-v">{{ m.v }}</span>
                </div>
                <div v-if="statsStale(d)" class="mt-stale">上次上报 {{ d.stats_at ? relTime(Math.floor(d.stats_at / 1000)) : "未知" }} · 已离线?</div>
              </div>
              <div class="dev-meters-none" v-else-if="!d.revoked">
                <Cpu :size="12" :stroke-width="1.9" /> 资源待上报(对方 App 登录后自动上报)
              </div>

              <div class="dev-grant" v-if="!d.revoked">
                <ShieldCheck :size="12" :stroke-width="2" />
                {{ d.is_host ? "本机主机 · 全权" : "已授权 · owner 完整权限" }}
              </div>
              <div class="dev-grant revoked" v-else>
                <ShieldOff :size="12" :stroke-width="2" /> 已吊销 · 无法再连入
              </div>

              <!-- 动作:本机无动作;远端可挂盘 / 派任务;owner 可吊销 -->
              <div class="dev-btns">
                <template v-if="d.is_host">
                  <span class="b flat">本机</span>
                </template>
                <template v-else-if="!d.revoked">
                  <button class="b" @click="mountDisk(d)"><FolderInput :size="13" /> 挂它的盘</button>
                  <button class="b pri" @click="openDispatch(d)"><Send :size="13" /> 派任务</button>
                  <button v-if="owner" class="b danger" title="吊销" @click="revoke(d)"><ShieldOff :size="13" /></button>
                </template>
                <template v-else>
                  <span class="b flat dim">已下线</span>
                </template>
              </div>
            </article>
          </div>

          <!-- 授权说明(PRD 原型④ 精神:精确到设备/权限/可吊销) -->
          <section class="glass grant-note">
            <div class="gn-head"><ShieldCheck :size="15" :stroke-width="1.9" /> 授权与安全</div>
            <ul class="gn-list">
              <li><b>同账号自己人免授权</b>:用账号密码登录即互信,直接挂盘、派任务。</li>
              <li><b>开给别人</b>要走「协作」定向邀请码,精确到资源、权限、期限,越权 fail-closed 拦截。</li>
              <li><b>随时可吊销</b>:点设备卡右侧吊销,该设备立刻断流、无法再连。</li>
            </ul>
          </section>
        </template>
        </template>

        <!-- 我共享出去的:开给别人的资源(走协作邀请码) -->
        <div v-else-if="devFilter === 'shared'" class="empty glass">
          还没有共享给别人的资源。到「协作」用定向邀请码把某个盘 / 项目开给指定的人,精确到权限与期限,这里就会留下记录。
        </div>

        <!-- 正在发生:audit 活动流(接入/上报/吊销/账号事件,主机留痕) -->
        <template v-else-if="devFilter === 'activity'">
          <div v-if="auditLoading" class="empty glass"><LoaderCircle :size="14" class="spin" /> 拉取活动流…</div>
          <div v-else-if="!auditRows.length" class="empty glass">
            还没有活动记录。设备登录、开始上报资源、被吊销等都会在这里留痕。
          </div>
          <section v-else class="glass act-card">
            <div v-for="(a, i) in auditRows" :key="i" class="act-row">
              <span class="act-dot" :class="'act-' + a.action.split('.')[0]"></span>
              <div class="act-main">
                <div class="act-title"><b>{{ a.actor }}</b> · {{ auditLabel(a) }}</div>
                <div class="act-sub">{{ a.target }}<template v-if="a.detail"> · {{ a.detail }}</template></div>
              </div>
              <span class="act-time">{{ relTime(a.at) }}</span>
            </div>
          </section>
        </template>
      </template>

      <!-- ════════════ ③ 网络拓扑 ════════════ -->
      <template v-else>
        <section class="glass topo-card">
          <div class="topo-head">
            <div>
              <div class="th-title"><Network :size="16" :stroke-width="1.9" /> 网络拓扑</div>
              <div class="th-sub">本机为心、设备为星。连线是<b>系统自动选的传输路径</b>,你无需选。</div>
            </div>
            <div class="topo-legend">
              <span class="lg t-lan"><i></i>局域网</span>
              <span class="lg t-p2p"><i></i>P2P</span>
              <span class="lg t-relay"><i></i>中继</span>
              <span class="lg t-disk"><i></i>远程盘</span>
            </div>
          </div>

          <div class="topo-stage">
            <svg :viewBox="`0 0 ${topo.W} ${topo.H}`" class="topo-svg" preserveAspectRatio="xMidYMid meet">
              <!-- 心跳光环 -->
              <circle :cx="topo.cx" :cy="topo.cy" :r="topo.R" class="ring" />
              <circle :cx="topo.cx" :cy="topo.cy" :r="topo.R * 0.62" class="ring faint" />

              <!-- 连线:流动虚线 = 数据在跑 -->
              <g v-for="(nd, i) in topo.nodes" :key="'edge' + i">
                <line
                  :x1="topo.cx" :y1="topo.cy" :x2="nd.x" :y2="nd.y"
                  class="edge" :class="TRANSPORT[nd.t].cls"
                />
                <line
                  :x1="topo.cx" :y1="topo.cy" :x2="nd.x" :y2="nd.y"
                  class="edge-flow" :class="TRANSPORT[nd.t].cls"
                />
              </g>

              <!-- 远端节点:入站设备 + 出站远程盘 -->
              <g v-for="(nd, i) in topo.nodes" :key="'node' + i" class="tnode" :class="{ off: nd.revoked }">
                <circle :cx="nd.x" :cy="nd.y" r="27" class="tn-halo" :class="TRANSPORT[nd.t].cls" />
                <circle :cx="nd.x" :cy="nd.y" r="21" class="tn-disc" />
                <text :x="nd.x" :y="nd.y - 34" class="tn-name" text-anchor="middle">{{ nd.name }}</text>
                <text :x="nd.x" :y="nd.y + 42" class="tn-badge" :class="TRANSPORT[nd.t].cls" text-anchor="middle">{{ TRANSPORT[nd.t].label }}</text>
                <text :x="nd.x" :y="nd.y + 5" class="tn-emoji" text-anchor="middle">{{ nd.emoji }}</text>
              </g>

              <!-- 中心:本机 -->
              <circle :cx="topo.cx" :cy="topo.cy" r="42" class="hub-halo" />
              <circle :cx="topo.cx" :cy="topo.cy" r="34" class="hub-disc" />
              <text :x="topo.cx" :y="topo.cy - 2" class="hub-emoji" text-anchor="middle">🖥</text>
              <text :x="topo.cx" :y="topo.cy + 15" class="hub-label" text-anchor="middle">本机</text>
              <text :x="topo.cx" :y="topo.cy + 58" class="hub-name" text-anchor="middle">{{ hostDevice?.name || (collab.user?.username ? '@' + collab.user.username : '这台电脑') }}</text>
            </svg>

            <div v-if="!topo.nodes.length" class="topo-empty">
              还没有远端节点。去「教程」用连接码把手机连进来,或在「文件中心 · 远程源」连一台 NAS —— 拓扑图上就会长出一颗星。
            </div>
          </div>

          <div class="topo-stats">
            <div class="ts"><span class="ts-n">{{ 1 + topo.nodes.length }}</span><span class="ts-l">节点</span></div>
            <div class="ts"><span class="ts-n">{{ remoteDevices.length }}</span><span class="ts-l">远端设备</span></div>
            <div class="ts"><span class="ts-n">{{ remotes.length }}</span><span class="ts-l">远程盘</span></div>
            <div class="ts"><span class="ts-n">{{ hasIroh ? 'P2P' : (remoteOn ? 'LAN' : '本机') }}</span><span class="ts-l">当前选档</span></div>
          </div>
        </section>
      </template>
    </div>
  </div>
</template>

<style scoped>
/* ═══════════ 玻璃琉璃基调 ═══════════
   苹果磨砂玻璃:半透明底 + backdrop blur + 顶缘高光 + 柔阴影。
   浅色顶缘白高光,深色压到几近无。 */
.interconnect {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-height: 0;
  --glass-bg: color-mix(in srgb, var(--panel) 68%, transparent);
  --glass-hi: rgba(255, 255, 255, 0.55);
  --glass-brd: color-mix(in srgb, var(--border) 60%, transparent);
}
:root[data-theme="dark"] .interconnect,
:root[data-theme="aurora-dark"] .interconnect {
  --glass-bg: color-mix(in srgb, var(--panel) 58%, transparent);
  --glass-hi: rgba(255, 255, 255, 0.07);
  --glass-brd: rgba(255, 255, 255, 0.09);
}
.glass {
  background: var(--glass-bg);
  backdrop-filter: blur(22px) saturate(180%);
  -webkit-backdrop-filter: blur(22px) saturate(180%);
  border: 1px solid var(--glass-brd);
  border-radius: 18px;
  box-shadow: var(--shadow-lg), inset 0 1px 0 var(--glass-hi);
}

.bar {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 11px 16px;
  border-bottom: 1px solid var(--hairline);
  flex-wrap: wrap;
}
.ttl { display: flex; align-items: center; gap: 8px; font-weight: 600; font-size: 15px; letter-spacing: .5px; }

/* 玻璃分段 Tab(仿手机「云端工作 / 连接电脑」药丸) */
.tabs {
  display: inline-flex;
  gap: 3px;
  padding: 4px;
  border-radius: 13px;
  background: color-mix(in srgb, var(--selection-bg) 55%, transparent);
  backdrop-filter: blur(12px);
  -webkit-backdrop-filter: blur(12px);
  border: 1px solid var(--glass-brd);
}
.tab {
  display: inline-flex; align-items: center; gap: 6px;
  padding: 7px 14px; border: none; background: none; cursor: pointer;
  border-radius: 10px; font-size: 12.5px; font-weight: 500; color: var(--muted);
  transition: color .18s, background .18s, box-shadow .18s;
}
.tab:hover { color: var(--text-2); }
.tab.on {
  background: var(--panel); color: var(--ink); font-weight: 600;
  box-shadow: var(--shadow), inset 0 1px 0 var(--glass-hi);
}
.icobtn { margin-left: auto; border: none; background: none; color: var(--muted); cursor: pointer; padding: 6px; border-radius: 8px; display: inline-flex; }
.icobtn:hover { color: var(--ink); background: var(--selection-bg); }
.icobtn.sm { padding: 4px; margin-left: auto; }
.scroll { flex: 1; overflow-y: auto; padding: 20px; display: flex; flex-direction: column; gap: 16px; max-width: 820px; width: 100%; margin: 0 auto; }

@media (max-width: 640px) {
  .tab-label { display: none; }
  .tab { padding: 7px 11px; }
}

.grad { background: linear-gradient(120deg, #0d99ff, #7c4dff); -webkit-background-clip: text; background-clip: text; -webkit-text-fill-color: transparent; }

/* ── 接入三步卡 ── */
.steps-card { padding: 24px 26px; position: relative; overflow: visible; }
.steps-card::before {
  content: ""; position: absolute; inset: 0; pointer-events: none; z-index: 0; border-radius: inherit;
  background: radial-gradient(120% 90% at 100% 0%, color-mix(in srgb, #7c4dff 12%, transparent), transparent 55%);
}
.sc-head, .steps { z-index: 1; }
.sc-head { position: relative; }
.sc-kick { font-size: 11.5px; font-weight: 700; letter-spacing: .16em; text-transform: uppercase; color: #0d99ff; }
.sc-title { font-size: 24px; margin: 8px 0 6px; letter-spacing: -.4px; line-height: 1.2; }
.sc-sub { margin: 0 0 4px; color: var(--text-2); font-size: 13px; line-height: 1.65; }
.steps { list-style: none; margin: 18px 0 0; padding: 0; display: grid; gap: 12px; position: relative; }
.steps li { display: flex; gap: 13px; align-items: flex-start; }
.st-n {
  flex: none; width: 26px; height: 26px; border-radius: 9px; color: #fff; font-weight: 700; font-size: 13px;
  display: flex; align-items: center; justify-content: center;
  background: linear-gradient(135deg, #0d99ff, #7c4dff);
  box-shadow: 0 4px 12px rgba(13, 153, 255, .3);
}
.steps li b { font-size: 14px; }
.steps li p { margin: 3px 0 0; font-size: 12.5px; color: var(--text-2); line-height: 1.6; }
.steps li div { min-width: 0; }

/* ① 主机连接卡(hero) */
.hero { padding: 20px 22px; }
.hero-head { display: flex; align-items: center; gap: 9px; font-weight: 600; font-size: 15.5px; margin-bottom: 10px; }
.hint { font-size: 13px; color: var(--text-2); line-height: 1.7; margin-bottom: 14px; }
.hint b { color: var(--ink); }
.code-box {
  border: 1px solid var(--glass-brd); border-radius: 12px;
  background: color-mix(in srgb, var(--bg) 55%, transparent); padding: 16px 14px; text-align: center;
  cursor: pointer; min-height: 56px; display: flex; align-items: center; justify-content: center;
  transition: border-color .15s;
}
.code-box:hover { border-color: #0d99ff; }
.code {
  font-family: var(--mono); font-size: 15px; font-weight: 600; letter-spacing: 1px;
  color: var(--ink); word-break: break-all; user-select: all; line-height: 1.5;
}
.code.dim { color: var(--muted); font-weight: 400; }
.code-actions { display: flex; gap: 10px; margin-top: 12px; }
.al { color: var(--muted); font-size: 11.5px; }
.field { margin-bottom: 14px; }
.fl { font-size: 12px; color: var(--muted); margin-bottom: 7px; font-weight: 500; }
.addr-list { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
.addr-list code { font-family: var(--mono); font-size: 12px; background: var(--selection-bg); padding: 4px 9px; border-radius: 6px; color: var(--text); user-select: all; }
.manual { margin-top: 6px; padding-top: 14px; border-top: 1px dashed var(--border); }
.lan-toggle {
  display: flex; align-items: center; gap: 12px; cursor: pointer;
  margin-top: 16px; padding: 12px 14px; border-radius: 12px;
  background: color-mix(in srgb, var(--bg) 55%, transparent); border: 1px solid var(--glass-brd);
}
.lan-toggle:hover { border-color: #0d99ff; }
.lan-toggle.busy { opacity: .6; pointer-events: none; }
.lt-txt { flex: 1; display: flex; flex-direction: column; gap: 3px; min-width: 0; }
.lt-title { font-weight: 600; font-size: 13.5px; }
.lt-sub { font-size: 11.5px; color: var(--muted); }
.switch {
  width: 44px; height: 26px; border-radius: 13px; flex: none; position: relative;
  background: var(--selection-bg); border: 1px solid var(--border); transition: background .15s;
}
.switch i { position: absolute; top: 2px; left: 2px; width: 20px; height: 20px; border-radius: 50%; background: var(--muted); transition: transform .15s, background .15s; }
.switch.on { background: #0d99ff; border-color: #0d99ff; }
.switch.on i { transform: translateX(18px); background: #fff; }
.code-box.sm { min-height: 40px; padding: 10px 12px; }
.code-box.sm .code { font-size: 12.5px; }
.foot-note { margin: 12px 0 0; font-size: 11.5px; color: var(--muted); line-height: 1.6; }
.foot-note b { color: var(--text-2); }
.cta {
  display: inline-flex; align-items: center; gap: 8px; justify-content: center;
  padding: 11px 18px; border-radius: 12px; border: none; cursor: pointer;
  background: linear-gradient(135deg, #0d99ff, #7c4dff); color: #fff; font-weight: 600; font-size: 14px;
  box-shadow: 0 6px 18px rgba(13, 153, 255, .28);
}
.cta.full { width: 100%; margin-top: 12px; }
.cta:active { transform: scale(.97); }
.cta:disabled { opacity: .6; cursor: not-allowed; }
.auth-form { display: flex; flex-direction: column; gap: 10px; max-width: 320px; }
.af-inp {
  border: 1px solid var(--glass-brd); border-radius: 10px;
  background: color-mix(in srgb, var(--bg) 55%, transparent); color: var(--ink);
  font-size: 13.5px; padding: 10px 12px; outline: none;
}
.af-inp:focus { border-color: #0d99ff; }
.af-err { margin: 0; font-size: 12px; color: var(--vermilion); }

/* pill 按钮 */
.pill {
  display: inline-flex; align-items: center; gap: 6px; justify-content: center;
  flex: 1; padding: 9px; border-radius: 10px; cursor: pointer; font-size: 13px;
  border: 1px solid transparent; background: #0d99ff; color: #fff;
}
.pill.ghost { background: color-mix(in srgb, var(--bg) 50%, transparent); color: var(--text); border-color: var(--glass-brd); }
.pill.ghost:hover { border-color: var(--ink); color: var(--ink); }
.pill:disabled { opacity: .5; cursor: not-allowed; }

/* ② 账号根卡 */
.rootcard { padding: 16px 18px; }
.rc-head { display: flex; align-items: center; gap: 8px; margin-bottom: 12px; }
.rc-title { font-weight: 600; font-size: 14.5px; }
.rc-badge { margin-left: auto; font-size: 11px; font-weight: 700; color: #fff; background: linear-gradient(135deg, #16a34a, #0d99ff); padding: 3px 10px; border-radius: 999px; }
.rc-code {
  font-family: var(--mono); font-size: 17px; letter-spacing: 1.5px; text-align: center;
  padding: 13px 8px; border-radius: 11px; background: color-mix(in srgb, var(--bg) 55%, transparent); border: 1px solid var(--glass-brd);
  user-select: all; word-break: break-all; cursor: pointer;
}
.rc-actions { display: flex; gap: 10px; margin-top: 11px; }

/* ── 设备联盟 ── */
.fed-head { display: flex; align-items: center; gap: 12px; padding: 13px 18px; }
.fh-av { width: 34px; height: 34px; border-radius: 50%; flex: none; background: linear-gradient(135deg, #0d99ff, #7c4dff); box-shadow: 0 4px 12px rgba(124, 77, 255, .3); }
.fh-txt { flex: 1; min-width: 0; }
.fh-title { font-weight: 600; font-size: 14.5px; }
.fh-sub { font-size: 11.5px; color: var(--muted); display: flex; align-items: center; gap: 6px; }
.odot { width: 7px; height: 7px; border-radius: 50%; background: #16a34a; display: inline-block; box-shadow: 0 0 0 3px color-mix(in srgb, #16a34a 22%, transparent); }

.empty { display: flex; align-items: center; gap: 8px; font-size: 12.5px; color: var(--muted); padding: 22px; justify-content: center; }

/* ── 设备子导航 ── */
.dev-subnav { display: flex; gap: 6px; flex-wrap: wrap; margin: 2px 0 2px; }
.dsn {
  display: inline-flex; align-items: center; gap: 6px; padding: 6px 13px; border-radius: 999px;
  font-size: 12.5px; font-weight: 600; color: var(--text-2); background: transparent;
  border: 1px solid var(--glass-brd); cursor: pointer; transition: all .16s var(--ease-out);
}
.dsn:hover { color: var(--text-1); background: color-mix(in srgb, var(--text-1) 5%, transparent); }
.dsn.on { color: #fff; background: linear-gradient(135deg, #0d99ff, #7c4dff); border-color: transparent; box-shadow: 0 3px 10px rgba(13, 153, 255, .28); }
.dsn-n { min-width: 17px; height: 17px; padding: 0 4px; border-radius: 9px; font-size: 10.5px; display: inline-flex; align-items: center; justify-content: center; background: color-mix(in srgb, currentColor 18%, transparent); }
.dsn.on .dsn-n { background: rgba(255,255,255,.28); }

/* ── 设备卡:核数 + 资源仪表 ── */
.dev-cores { font-size: 10.5px; font-weight: 700; color: var(--text-2); padding: 1px 7px; border-radius: 6px; background: color-mix(in srgb, var(--text-1) 7%, transparent); }
.dev-meters { display: grid; gap: 7px; margin: 4px 0 10px; }
.mt { display: grid; grid-template-columns: 30px 1fr auto; align-items: center; gap: 8px; }
.mt-k { font-size: 10.5px; color: var(--muted); }
.mt-bar { height: 6px; border-radius: 4px; background: color-mix(in srgb, var(--text-1) 9%, transparent); overflow: hidden; }
.mt-bar i { display: block; height: 100%; border-radius: 4px; transition: width .5s var(--ease-out); }
.mt-bar i.m-cool { background: linear-gradient(90deg, #0d99ff, #2ec5ff); }
.mt-bar i.m-warm { background: linear-gradient(90deg, #d97706, #f0a53b); }
.mt-bar i.m-hot { background: linear-gradient(90deg, #dc2626, #f2603b); }
.mt-v { font-family: var(--mono); font-size: 10px; color: var(--text-2); min-width: 44px; text-align: right; }
.dev-meters-none { display: flex; align-items: center; gap: 6px; font-size: 11px; color: var(--muted); margin: 6px 0 10px; }
.dev-meters.stale { opacity: .45; }
.mt-stale { font-size: 10px; color: var(--muted); }
.remote-block { padding: 4px 0 8px; border-bottom: 1px dashed var(--glass-brd); }
.remote-block:last-of-type { border-bottom: none; }
.remote-block .dev-meters { max-width: 420px; margin: 8px 0 2px; }

/* ── 正在发生(活动流) ── */
.act-card { padding: 8px 16px; }
.act-row { display: flex; align-items: center; gap: 11px; padding: 10px 2px; border-bottom: 1px solid var(--glass-brd); }
.act-row:last-child { border-bottom: none; }
.act-dot { width: 8px; height: 8px; border-radius: 50%; background: #8a8f98; flex: none; }
.act-dot.act-auth { background: #16a34a; }
.act-dot.act-device { background: #0d99ff; }
.act-dot.act-user { background: #7c4dff; }
.act-dot.act-mirror { background: #d97706; }
.act-main { flex: 1; min-width: 0; }
.act-title { font-size: 13px; }
.act-sub { font-size: 11.5px; color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.act-time { font-size: 11px; color: var(--muted); flex: none; }

.dev-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(238px, 1fr)); gap: 13px; }
.dev { padding: 15px 16px; position: relative; overflow: hidden; transition: transform .18s var(--ease-out), box-shadow .18s; }
.dev:hover { transform: translateY(-2px); box-shadow: var(--shadow-lg), inset 0 1px 0 var(--glass-hi), 0 14px 30px rgba(20, 20, 25, .08); }
.dev::after { content: ""; position: absolute; left: 0; right: 0; top: 0; height: 3px; opacity: .85; }
.dev.t-local::after { background: linear-gradient(90deg, #8a8f98, #b5b9c0); }
.dev.t-lan::after { background: linear-gradient(90deg, #16a34a, #37c76a); }
.dev.t-p2p::after { background: linear-gradient(90deg, #0d99ff, #2ec5ff); }
.dev.t-relay::after { background: linear-gradient(90deg, #d97706, #f0a53b); }
.dev.off { opacity: .5; }
.dev.off::after { background: var(--border); }
.dev-top { display: flex; align-items: center; gap: 10px; }
.dev-ico { width: 36px; height: 36px; border-radius: 10px; background: color-mix(in srgb, var(--selection-bg) 70%, transparent); display: flex; align-items: center; justify-content: center; color: var(--ink); flex: none; }
.dev-id { flex: 1; min-width: 0; }
.dev-name { font-weight: 600; font-size: 13.5px; display: flex; align-items: center; gap: 6px; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.dev-owner { font-size: 11px; color: var(--muted); }
.host-badge { font-size: 10px; font-weight: 700; color: #b8860b; background: color-mix(in srgb, #b8860b 14%, transparent); border-radius: 4px; padding: 1px 6px; flex: none; }
.dev-dot { width: 8px; height: 8px; border-radius: 50%; background: var(--muted); flex: none; }
.dev-dot.on { background: #16a34a; box-shadow: 0 0 0 3px color-mix(in srgb, #16a34a 20%, transparent); }
.dev-line { display: flex; align-items: center; gap: 8px; margin: 13px 0 8px; }
.conn { display: inline-flex; align-items: center; gap: 4px; font-size: 10.5px; font-weight: 700; padding: 3px 9px; border-radius: 7px; }
.conn.t-local { background: color-mix(in srgb, #8a8f98 16%, transparent); color: var(--text-2); }
.conn.t-lan { background: color-mix(in srgb, #16a34a 15%, transparent); color: #16a34a; }
.conn.t-p2p { background: color-mix(in srgb, #0d99ff 15%, transparent); color: #0d99ff; }
.conn.t-relay { background: color-mix(in srgb, #d97706 16%, transparent); color: #d97706; }
.dev-node { margin-left: auto; font-family: var(--mono); font-size: 10.5px; color: var(--muted); }
.dev-grant { display: flex; align-items: center; gap: 5px; font-size: 11px; color: var(--text-2); margin-bottom: 12px; }
.dev-grant.revoked { color: var(--vermilion); }
.dev-btns { display: flex; gap: 7px; }
.b {
  flex: 1; display: inline-flex; align-items: center; justify-content: center; gap: 5px;
  font-size: 12px; padding: 8px; border-radius: 9px; cursor: pointer;
  border: 1px solid var(--glass-brd); background: color-mix(in srgb, var(--bg) 45%, transparent); color: var(--text);
  transition: border-color .15s, background .15s;
}
.b:hover { border-color: var(--ink); }
.b.pri { background: linear-gradient(135deg, #0d99ff, #7c4dff); color: #fff; border-color: transparent; box-shadow: 0 4px 12px rgba(13, 153, 255, .26); }
.b.pri:hover { filter: brightness(1.05); }
.b.danger { flex: none; width: 34px; color: var(--vermilion); }
.b.danger:hover { border-color: var(--vermilion); background: var(--vermilion-soft); }
.b.flat { cursor: default; background: none; border-color: transparent; color: var(--muted); }
.b.flat.dim { color: var(--dim); }

.grant-note { padding: 16px 18px; }
.gn-head { display: flex; align-items: center; gap: 8px; font-weight: 600; font-size: 13.5px; margin-bottom: 10px; }
.gn-list { margin: 0; padding-left: 20px; display: grid; gap: 6px; }
.gn-list li { font-size: 12.5px; color: var(--text-2); line-height: 1.6; }
.gn-list b { color: var(--ink); }
.remote-line { margin: 10px 0 0; padding: 8px 10px; border-radius: 10px; background: color-mix(in srgb, var(--bg) 45%, transparent); border: 1px solid var(--glass-brd); }
.remote-line .b.danger { margin-left: 4px; }

/* ── 网络拓扑 ── */
.topo-card { padding: 18px 20px; }
.topo-head { display: flex; align-items: flex-start; justify-content: space-between; gap: 14px; flex-wrap: wrap; margin-bottom: 8px; }
.th-title { display: flex; align-items: center; gap: 7px; font-weight: 600; font-size: 15px; }
.th-sub { font-size: 12px; color: var(--muted); margin-top: 3px; }
.th-sub b { color: var(--text-2); }
.topo-legend { display: flex; gap: 12px; }
.lg { display: inline-flex; align-items: center; gap: 5px; font-size: 11px; color: var(--muted); }
.lg i { width: 9px; height: 9px; border-radius: 2px; display: inline-block; }
.lg.t-lan i { background: #16a34a; } .lg.t-p2p i { background: #0d99ff; } .lg.t-relay i { background: #d97706; } .lg.t-disk i { background: #14b8a6; }

.topo-stage { position: relative; margin: 6px 0 4px; }
.topo-svg { width: 100%; height: auto; display: block; overflow: visible; }
.ring { fill: none; stroke: var(--glass-brd); stroke-width: 1; stroke-dasharray: 3 5; opacity: .7; }
.ring.faint { opacity: .4; }
.edge { stroke-width: 2; opacity: .35; }
.edge.t-lan { stroke: #16a34a; } .edge.t-p2p { stroke: #0d99ff; } .edge.t-relay { stroke: #d97706; } .edge.t-disk { stroke: #14b8a6; }
.edge-flow { stroke-width: 2.4; stroke-dasharray: 2 12; stroke-linecap: round; animation: flow 1.1s linear infinite; }
.edge-flow.t-lan { stroke: #16a34a; } .edge-flow.t-p2p { stroke: #0d99ff; } .edge-flow.t-relay { stroke: #d97706; } .edge-flow.t-disk { stroke: #14b8a6; }
@keyframes flow { to { stroke-dashoffset: -14; } }

.hub-halo { fill: color-mix(in srgb, #7c4dff 18%, transparent); animation: pulse 2.6s ease-in-out infinite; }
.hub-disc { fill: var(--panel); stroke: #7c4dff; stroke-width: 1.5; filter: drop-shadow(0 4px 12px rgba(124, 77, 255, .35)); }
.hub-emoji { font-size: 22px; }
.hub-label { font-size: 10px; font-weight: 700; fill: #7c4dff; }
.hub-name { font-size: 11px; fill: var(--muted); }
@keyframes pulse { 0%, 100% { transform: scale(1); opacity: .6; } 50% { transform: scale(1.14); opacity: .3; } }
.hub-halo { transform-box: fill-box; transform-origin: center; }

.tnode.off { opacity: .45; }
.tn-halo { fill: none; stroke-width: 1.5; opacity: .5; }
.tn-halo.t-lan { stroke: #16a34a; } .tn-halo.t-p2p { stroke: #0d99ff; } .tn-halo.t-relay { stroke: #d97706; } .tn-halo.t-disk { stroke: #14b8a6; }
.tn-disc { fill: var(--panel); stroke: var(--glass-brd); stroke-width: 1; filter: drop-shadow(0 3px 8px rgba(20, 20, 25, .12)); }
.tn-emoji { font-size: 17px; }
.tn-name { font-size: 11px; font-weight: 600; fill: var(--text); }
.tn-badge { font-size: 9.5px; font-weight: 700; }
.tn-badge.t-lan { fill: #16a34a; } .tn-badge.t-p2p { fill: #0d99ff; } .tn-badge.t-relay { fill: #d97706; } .tn-badge.t-disk { fill: #14b8a6; }

.topo-empty { position: absolute; left: 50%; bottom: 8px; transform: translateX(-50%); text-align: center; font-size: 12px; color: var(--muted); max-width: 320px; }

.topo-stats { display: grid; grid-template-columns: repeat(4, 1fr); gap: 10px; margin-top: 14px; padding-top: 14px; border-top: 1px solid var(--hairline); }
.ts { text-align: center; }
.ts-n { display: block; font-size: 19px; font-weight: 700; color: var(--ink); font-family: var(--mono); }
.ts-l { font-size: 11px; color: var(--muted); }

/* ── 派任务浮层 ── */
.dispatch-mask {
  position: fixed; inset: 0; z-index: 2000; display: flex; align-items: center; justify-content: center;
  background: var(--overlay); backdrop-filter: blur(3px); -webkit-backdrop-filter: blur(3px);
  animation: fade .18s ease;
}
@keyframes fade { from { opacity: 0; } }
.dispatch { width: min(420px, calc(100vw - 40px)); padding: 18px 20px; animation: pop .22s var(--ease-spring); }
@keyframes pop { from { transform: scale(.94); opacity: 0; } }
.dp-head { display: flex; align-items: center; gap: 11px; margin-bottom: 16px; }
.dp-ico { width: 38px; height: 38px; border-radius: 11px; background: color-mix(in srgb, var(--selection-bg) 70%, transparent); display: flex; align-items: center; justify-content: center; color: var(--ink); flex: none; }
.dp-title { font-weight: 600; font-size: 15px; }
.dp-sub { font-size: 11.5px; color: var(--muted); }
.dp-head .icobtn { font-size: 15px; }
.dp-connecting { display: flex; flex-direction: column; align-items: center; gap: 12px; padding: 26px 0; color: var(--text-2); }
.dp-c-txt { text-align: center; font-size: 13.5px; line-height: 1.5; }
.dp-c-txt span { font-size: 11.5px; color: var(--muted); }
.dp-conn-ok { display: flex; align-items: center; gap: 7px; font-size: 13px; color: #16a34a; font-weight: 600; margin-bottom: 12px; }
.dp-conn-ok .conn { font-weight: 700; }
.dp-input {
  width: 100%; border: 1px solid var(--glass-brd); border-radius: 12px; resize: vertical;
  background: color-mix(in srgb, var(--bg) 55%, transparent); color: var(--ink);
  font-family: inherit; font-size: 13.5px; padding: 12px 13px; outline: none; line-height: 1.6;
}
.dp-input:focus { border-color: #0d99ff; }
.dp-note { margin: 10px 0 0; font-size: 11px; color: var(--muted); line-height: 1.6; }
.dp-sent { display: flex; flex-direction: column; align-items: center; gap: 8px; padding: 24px 0; color: #16a34a; }
.dp-sent div { font-size: 15px; font-weight: 600; }
.dp-sent span { font-size: 11.5px; color: var(--muted); }

.dim { font-size: 11.5px; color: var(--dim); }
.spin { animation: spin .9s linear infinite; }
@keyframes spin { to { transform: rotate(360deg); } }
</style>

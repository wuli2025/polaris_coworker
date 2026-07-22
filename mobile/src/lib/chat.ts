/**
 * 聊天 —— Kimi 式核心。基于 net.invoke("chat_send") + listen("chat:stream")。
 *
 * 流式事件 {kind, conversationId, text, tool}:
 *  - delta    → 逐字追加到当前助手气泡(40ms 攒批落地,防高频重渲染)
 *  - tool     → 一条工具调用气泡
 *  - artifact → 记到当前助手气泡的产物列表
 *  - error    → 错误气泡
 *  - done     → 本轮结束
 *
 * 历史对话:按「当前主机」本地持久化(convs.ts),消息变更 800ms 合并落盘。
 * 手机端只存文本与产物路径 —— 文件本体永远在主机。
 */
import { reactive, ref } from "vue";
import { invoke, listen } from "./net";
import { activeHostId } from "./hosts";
import { saveConv, loadConv, deleteConv, listConvs, type ConvMeta } from "./convs";
import { startTelemetry, stopTelemetry } from "./telemetry";
import { resetPickers } from "./pickers";

export interface Bubble {
  role: "user" | "assistant" | "tool" | "error";
  text: string;
  tool?: string;
  artifacts?: string[];
  at: number;
}

interface StreamEvent {
  kind: "delta" | "tool" | "artifact" | "error" | "done";
  conversationId?: string;
  text?: string;
  tool?: string;
}

export const messages = reactive<Bubble[]>([]);
export const sending = ref(false);
export const convId = ref<string>(`m-${Date.now()}`);
/** 抽屉里的历史列表(响应式镜像,persist 后刷新)。 */
export const convList = ref<ConvMeta[]>(listConvs(activeHostId.value));

// ── 主机上的全部对话(登录后拉取;手机/桌面发起的都在) ──
// chat_send 服务端会把任意 conversationId 登记进 conv 表,所以这份列表 = 全部对话的真相;
// 本地 convs.ts 退化为离线缓存(断网也能翻)。
export interface RemoteConv {
  id: string;
  title: string;
  at: number;
  project: string;
}
export const remoteConvs = ref<RemoteConv[]>([]);
export const remoteLoading = ref(false);

interface RProject {
  id: string;
  name: string;
}
interface RConv {
  id: string;
  title: string;
  updated_at: number;
}
interface RMessage {
  role: string;
  content: string;
  created_at: number;
}

// 拉取代际:切主机 / 重载即 +1,让在途的旧主机拉取结果作废,不串台(codex #7)。
let convFetchGen = 0;
/** 拉主机全部对话(各项目并行,新→旧,封顶 120 条)。失败静默保持现状(离线可用)。 */
export async function refreshRemoteConvs(): Promise<void> {
  // 不用 remoteLoading 早退挡并发 —— 切主机会立刻发起新一轮,早退会让新主机拉不到;
  // 改由 convFetchGen 代际裁决:新一轮作废旧的,flag 归最新那轮收尾。
  remoteLoading.value = true;
  const myGen = ++convFetchGen;
  const forHost = liveHost;
  try {
    const projects = await invoke<RProject[]>("conv_list_projects");
    const lists = await Promise.all(
      (projects ?? []).map((p) =>
        invoke<RConv[]>("conv_list_conversations", { projectId: p.id })
          .then((cs) =>
            (cs ?? []).map((c) => ({
              id: c.id,
              title: c.title || "未命名对话",
              at: c.updated_at,
              project: p.name,
            }))
          )
          .catch(() => [] as RemoteConv[])
      )
    );
    // 拉取期间切了主机 / 又发起了新一轮 → 陈旧结果丢弃,别覆盖当前主机的列表。
    if (myGen !== convFetchGen || liveHost !== forHost) return;
    remoteConvs.value = lists
      .flat()
      .sort((a, b) => b.at - a.at)
      .slice(0, 120);
  } catch {
    /* 主机老版本没这命令 / 离线:抽屉退回本地缓存 */
  } finally {
    if (myGen === convFetchGen) remoteLoading.value = false;
  }
}

/** 打开主机上的对话:本地缓存先上屏(秒开),再拉全量覆盖并回写缓存;可直接续聊。 */
export async function openHostConversation(id: string): Promise<void> {
  if (id === convId.value) return;
  flushPersist();
  clearLive();
  convId.value = id;
  const forHost = liveHost; // 捕获发起时的主机
  const cached = loadConv(liveHost, id);
  for (const m of cached) messages.push(m);
  try {
    const msgs = await invoke<RMessage[]>("conv_get_messages", { conversationId: id });
    // 等待期间用户切了会话或换了主机 → 陈旧响应丢弃,别覆盖当前(codex #8)。
    if (convId.value !== id || liveHost !== forHost) return;
    if (!msgs || !msgs.length) return; // 主机无此对话/空 → 保留缓存,别清屏(codex #9)
    messages.splice(0, messages.length);
    for (const m of msgs) {
      const role: Bubble["role"] =
        m.role === "user" ? "user" : m.role === "tool" ? "tool" : "assistant";
      messages.push({ role, text: m.content, at: m.created_at });
    }
    schedulePersist(); // 回写本地缓存,下次离线也能看
  } catch {
    /* 拉不到就用缓存,不打断 */
  }
}

let unlisten: (() => void) | null = null;
let reqId: string | null = null;
// 代数守卫:取消/切会话时 +1。chat_send 尚未 resolve 用户就取消的话,
// resolve 回来发现代数已变 → 立即补一刀 chat_cancel,不留孤儿请求。
let gen = 0;
let pending = "";
// pending 缓冲所属的会话:flush 时会话已切换则丢弃,防旧会话文字漏进新会话。
let pendingCid = "";
let flushTimer: ReturnType<typeof setTimeout> | null = null;

// ── 本地持久化(800ms 合并) ────────────────────────────
let saveTimer: ReturnType<typeof setTimeout> | null = null;
// 当前消息所属的主机:落盘必须用它而非 activeHostId —— 切主机时 activateHost
// 先行,activeHostId 已指向新主机,若此刻还按 activeHostId 落盘,旧主机的
// 对话会串进新主机的历史。
let liveHost = activeHostId.value;

function convTitle(): string {
  const first = messages.find((m) => m.role === "user");
  return (first?.text ?? "新对话").slice(0, 40);
}
function persistNow(): void {
  saveTimer = null;
  if (!messages.length) return;
  saveConv(liveHost, convId.value, convTitle(), messages.slice());
  if (liveHost === activeHostId.value) convList.value = listConvs(liveHost);
}
function schedulePersist(): void {
  if (!saveTimer) saveTimer = setTimeout(persistNow, 800);
}
/** 切会话/切主机前的立即落盘(把 800ms 窗口内的尾巴收掉)。 */
function flushPersist(): void {
  if (saveTimer) {
    clearTimeout(saveTimer);
    persistNow();
  }
}

function appendDelta(text: string) {
  if (!text) return;
  const last = messages[messages.length - 1];
  if (last && last.role === "assistant") last.text += text;
  else messages.push({ role: "assistant", text, at: Date.now() });
  schedulePersist();
}
function flush() {
  flushTimer = null;
  const t = pending;
  pending = "";
  if (t && pendingCid === convId.value) appendDelta(t);
}

/** app 级初始化:注册一次流式监听 + 拉一次主机对话列表 + 启动遥测上报。 */
export function initChat(): void {
  if (!remoteConvs.value.length) refreshRemoteConvs(); // 冷启动续会话也要有主机列表
  startTelemetry(); // 幂等;登录后手机开始把自己的资源报给主机(设备看板仪表变真)
  if (unlisten) return;
  unlisten = listen<StreamEvent>("chat:stream", (ev) => {
    if (ev.conversationId && ev.conversationId !== convId.value) return;
    if (ev.kind === "delta") {
      pendingCid = ev.conversationId ?? convId.value;
      pending += ev.text ?? "";
      if (!flushTimer) flushTimer = setTimeout(flush, 40);
      return;
    }
    flush();
    if (ev.kind === "tool") {
      messages.push({
        role: "tool",
        text: `调用工具:${ev.tool ?? "(unknown)"}`,
        tool: ev.tool,
        at: Date.now(),
      });
    } else if (ev.kind === "artifact") {
      const path = ev.text;
      if (path) {
        let target: Bubble | undefined;
        for (let i = messages.length - 1; i >= 0; i--) {
          if (messages[i].role === "assistant") {
            target = messages[i];
            break;
          }
        }
        if (!target) {
          target = { role: "assistant", text: "", artifacts: [], at: Date.now() };
          messages.push(target);
        }
        (target.artifacts ??= []).push(path);
      }
    } else if (ev.kind === "error") {
      messages.push({ role: "error", text: ev.text ?? "出错了", at: Date.now() });
      sending.value = false;
      reqId = null;
    } else if (ev.kind === "done") {
      sending.value = false;
      reqId = null;
    }
    schedulePersist();
  });
}

export interface SendOpts {
  attachments?: string[];
  /** "fast"(默认·省略) | "work" —— 输入条「深度工作」开关。 */
  workMode?: string;
  /** 供应商 id;空/"auto" = 跟随主机全局,不下发字段。 */
  providerId?: string;
  /** 输入条勾选的技能 id 列表。 */
  skillIds?: string[];
}

export async function sendMessage(prompt: string, opts?: SendOpts): Promise<void> {
  const text = prompt.trim();
  if (!text || sending.value) return;
  initChat();
  messages.push({ role: "user", text, at: Date.now() });
  schedulePersist();
  sending.value = true;
  const myGen = ++gen;
  try {
    // 附件不进 chat_send(ChatSendArgs 没有 attachments 字段,传了会被静默丢弃)——
    // 与桌面一致:先 chat_attach_files 挂到会话,chat_send 时服务端自取。
    if (opts?.attachments?.length) {
      await invoke("chat_attach_files", {
        conversationId: convId.value,
        paths: opts.attachments,
      });
    }
    // chat_send 是服务端唯一「具名参数」命令:实参要再包一层 args(server.rs 取
    // args.args 反序列化 ChatSendArgs);permissionMode 无 serde default 必填,
    // 手机端没有权限确认 UI,固定 auto_current(=acceptEdits,与桌面自动化路径一致)。
    // workMode/providerId/skillIds 有 serde default,老主机没有的字段会被忽略,兼容。
    const sendArgs: Record<string, unknown> = {
      prompt: text,
      conversationId: convId.value,
      permissionMode: "auto_current",
    };
    if (opts?.workMode && opts.workMode !== "fast") sendArgs.workMode = opts.workMode;
    if (opts?.providerId && opts.providerId !== "auto") sendArgs.providerId = opts.providerId;
    if (opts?.skillIds?.length) sendArgs.skillIds = opts.skillIds;
    const rid = await invoke<string>("chat_send", { args: sendArgs });
    if (myGen !== gen) {
      // 用户已在 send resolve 前取消/切会话 → 这个请求是孤儿,立即补杀。
      if (typeof rid === "string") invoke("chat_cancel", { reqId: rid }).catch(() => {});
      return;
    }
    reqId = typeof rid === "string" ? rid : null;
  } catch (e) {
    if (myGen !== gen) return; // 已取消/已开新轮:陈旧失败不再污染当前状态
    messages.push({
      role: "error",
      text: `[发送失败] ${(e as Error).message}`,
      at: Date.now(),
    });
    sending.value = false;
    schedulePersist();
  }
}

export async function cancel(): Promise<void> {
  gen++; // 让在途 chat_send 的 resolve 自知作废
  if (reqId) {
    try {
      await invoke("chat_cancel", { reqId });
    } catch {
      /* ignore */
    }
  }
  reqId = null;
  sending.value = false;
}

/** 清空当前会话状态(不落盘、不改索引)—— 内部复用。 */
function clearLive(): void {
  cancel();
  pending = "";
  pendingCid = "";
  if (flushTimer) {
    clearTimeout(flushTimer);
    flushTimer = null;
  }
  messages.splice(0, messages.length);
}

export function newConversation(): void {
  flushPersist(); // 旧会话先收尾落盘
  clearLive();
  convId.value = `m-${Date.now()}`;
}

/** 打开一条历史会话(从本地恢复消息;继续聊会沿用该 conversationId)。 */
export function openConversation(id: string): void {
  if (id === convId.value) return;
  flushPersist();
  clearLive();
  convId.value = id;
  const restored = loadConv(liveHost, id);
  for (const m of restored) messages.push(m);
}

/** 删除一条历史会话;若删的是当前会话,顺手开新。 */
export function removeConversation(id: string): void {
  deleteConv(liveHost, id);
  convList.value = listConvs(liveHost);
  if (id === convId.value) {
    clearLive();
    convId.value = `m-${Date.now()}`;
  }
}

/** 切主机后的会话侧重载:旧主机对话先落盘,再换历史列表 + 开新会话。 */
export function reloadForHost(): void {
  flushPersist(); // 仍按旧 liveHost 落盘
  liveHost = activeHostId.value;
  clearLive();
  convId.value = `m-${Date.now()}`;
  convList.value = listConvs(liveHost);
  remoteConvs.value = []; // 旧主机的列表不跨主机残留
  resetPickers(); // 供应商/技能选中态不跨主机残留(上家的 id 在下家无意义)
  refreshRemoteConvs(); // 登录/切主机后拉主机全部对话(异步,不阻塞进屏)
}

/** 登出时的全量复位:清消息、杀在途请求、退订流式监听、停遥测。 */
export function resetChat(): void {
  flushPersist();
  clearLive();
  convId.value = `m-${Date.now()}`;
  stopTelemetry();
  if (unlisten) {
    unlisten();
    unlisten = null;
  }
}

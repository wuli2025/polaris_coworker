/**
 * 聊天 —— Kimi 式核心。基于 net.invoke("chat_send") + listen("chat:stream")。
 *
 * 流式事件 {kind, conversationId, text, tool}:
 *  - delta    → 逐字追加到当前助手气泡(40ms 攒批落地,防高频重渲染)
 *  - tool     → 一条工具调用气泡
 *  - artifact → 记到当前助手气泡的产物列表
 *  - error    → 错误气泡
 *  - done     → 本轮结束
 */
import { reactive, ref } from "vue";
import { invoke, listen } from "./net";

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

let unlisten: (() => void) | null = null;
let reqId: string | null = null;
// 代数守卫:取消/切会话时 +1。chat_send 尚未 resolve 用户就取消的话,
// resolve 回来发现代数已变 → 立即补一刀 chat_cancel,不留孤儿请求。
let gen = 0;
let pending = "";
// pending 缓冲所属的会话:flush 时会话已切换则丢弃,防旧会话文字漏进新会话。
let pendingCid = "";
let flushTimer: ReturnType<typeof setTimeout> | null = null;

function appendDelta(text: string) {
  if (!text) return;
  const last = messages[messages.length - 1];
  if (last && last.role === "assistant") last.text += text;
  else messages.push({ role: "assistant", text, at: Date.now() });
}
function flush() {
  flushTimer = null;
  const t = pending;
  pending = "";
  if (t && pendingCid === convId.value) appendDelta(t);
}

/** app 级初始化:注册一次流式监听。返回就绪 promise。 */
export function initChat(): void {
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
  });
}

export async function sendMessage(
  prompt: string,
  opts?: { attachments?: string[] }
): Promise<void> {
  const text = prompt.trim();
  if (!text || sending.value) return;
  initChat();
  messages.push({ role: "user", text, at: Date.now() });
  sending.value = true;
  const myGen = ++gen;
  try {
    const rid = await invoke<string>("chat_send", {
      prompt: text,
      conversationId: convId.value,
      ...(opts?.attachments?.length ? { attachments: opts.attachments } : {}),
    });
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

export function newConversation(): void {
  cancel();
  // 清掉旧会话残留的流式缓冲,防止 40ms 窗口内的旧文字落进新会话
  pending = "";
  pendingCid = "";
  if (flushTimer) {
    clearTimeout(flushTimer);
    flushTimer = null;
  }
  messages.splice(0, messages.length);
  convId.value = `m-${Date.now()}`;
}

/** 登出时的全量复位:清消息、杀在途请求、退订流式监听。 */
export function resetChat(): void {
  newConversation();
  if (unlisten) {
    unlisten();
    unlisten = null;
  }
}

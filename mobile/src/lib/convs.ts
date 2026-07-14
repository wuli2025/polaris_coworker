/**
 * 历史对话本地持久化 —— 按主机分开存(切主机不串台)。
 *
 * 布局:索引 + 逐会话两级键,避免单键巨块反复整写:
 *   polaris.m.convidx.<hostId>       → ConvMeta[](新→旧)
 *   polaris.m.conv.<hostId>.<convId> → Bubble[]
 *
 * 容量纪律(手机端只保留"对话的样子",不是档案库):
 *   每主机最多 MAX_CONVS 条会话,超出淘汰最旧;每会话最多 MAX_MSGS 条消息,砍头留尾。
 *   localStorage 写满(QuotaExceeded)时按同一顺序淘汰后重试。
 */
import type { Bubble } from "./chat";

export interface ConvMeta {
  id: string;
  title: string;
  at: number; // 最后更新时间
}

const MAX_CONVS = 50;
const MAX_MSGS = 300;

const idxKey = (hostId: string) => `polaris.m.convidx.${hostId}`;
const convKey = (hostId: string, id: string) => `polaris.m.conv.${hostId}.${id}`;

export function listConvs(hostId: string): ConvMeta[] {
  if (!hostId) return [];
  try {
    const v = JSON.parse(localStorage.getItem(idxKey(hostId)) ?? "[]");
    return Array.isArray(v) ? v : [];
  } catch {
    return [];
  }
}

export function loadConv(hostId: string, id: string): Bubble[] {
  try {
    const v = JSON.parse(localStorage.getItem(convKey(hostId, id)) ?? "[]");
    return Array.isArray(v) ? v : [];
  } catch {
    return [];
  }
}

function writeIdx(hostId: string, idx: ConvMeta[]): void {
  localStorage.setItem(idxKey(hostId), JSON.stringify(idx));
}

/** 保存(新会话自动入索引并置顶;空消息列表则忽略)。 */
export function saveConv(hostId: string, id: string, title: string, messages: Bubble[]): void {
  if (!hostId || !messages.length) return;
  const trimmed = messages.length > MAX_MSGS ? messages.slice(-MAX_MSGS) : messages;
  let idx = listConvs(hostId).filter((m) => m.id !== id);
  idx.unshift({ id, title: title || "新对话", at: Date.now() });
  // 淘汰最旧
  for (const dead of idx.slice(MAX_CONVS)) {
    localStorage.removeItem(convKey(hostId, dead.id));
  }
  idx = idx.slice(0, MAX_CONVS);
  try {
    localStorage.setItem(convKey(hostId, id), JSON.stringify(trimmed));
    writeIdx(hostId, idx);
  } catch {
    // 空间不够:从最旧开始腾,腾一条试一次
    for (let i = idx.length - 1; i > 0; i--) {
      const dead = idx.pop()!;
      localStorage.removeItem(convKey(hostId, dead.id));
      try {
        localStorage.setItem(convKey(hostId, id), JSON.stringify(trimmed));
        writeIdx(hostId, idx);
        return;
      } catch {
        /* 继续腾 */
      }
    }
  }
}

export function deleteConv(hostId: string, id: string): void {
  localStorage.removeItem(convKey(hostId, id));
  writeIdx(
    hostId,
    listConvs(hostId).filter((m) => m.id !== id)
  );
}

/** 相对时间(抽屉列表用)。 */
export function relTime(at: number): string {
  const d = Date.now() - at;
  if (d < 60_000) return "刚刚";
  if (d < 3_600_000) return `${Math.floor(d / 60_000)} 分钟前`;
  if (d < 86_400_000) return `${Math.floor(d / 3_600_000)} 小时前`;
  if (d < 7 * 86_400_000) return `${Math.floor(d / 86_400_000)} 天前`;
  const t = new Date(at);
  return `${t.getMonth() + 1}/${t.getDate()}`;
}

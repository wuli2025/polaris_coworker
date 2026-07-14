/**
 * 多主机管理 —— UU远程式「设备列表」。
 *
 * 每台主机一条 HostEntry:地址候选(分享码可带多个)、各自的登录态(token+user)。
 * 「当前主机」写回 net.ts 的单例 BASE/TOKEN/USER 键 —— net 层继续只认单一会话,
 * 切主机 = 换写这三个键 + resetWs,对 net/chat 全透明。
 *
 * 手机端只存:主机列表 + 各主机会话 + 历史对话文本。不存任何远端文件。
 */
import { ref } from "vue";
import {
  setBase,
  setToken,
  setUser,
  getBase,
  getToken,
  getUser,
  probe,
  resetWs,
  type CollabUser,
} from "./net";
import { tunnelAvailable, startTunnel, stopTunnel, tunnelState } from "./tunnel";

export interface HostEntry {
  id: string;
  name: string; // 用户起的名,默认取地址
  base: string; // 上次连通的地址
  addrs: string[]; // 候选地址(分享码可有多个),探活时逐个试
  /** 主机 iroh NodeId(路线 B:P2P 直连)。有它且原生隧道可用 → 优先起隧道直连桌面,
   *  失败再回落 addrs(局域网 IP / 云网关中继地址)。 */
  nodeId?: string;
  token?: string; // 该主机上的登录态
  user?: CollabUser | null;
  /** 记住的账号密码(用户勾选「记住」):token 失效时静默重登,不再打断。
   *  明文存本机 localStorage —— 手机是私人设备,用户明确要求「记住账号密码」。 */
  savedUsername?: string;
  savedPassword?: string;
  lastUsed: number;
}

const HOSTS_KEY = "polaris.m.hosts";
const ACTIVE_KEY = "polaris.m.activeHost";

function load(): HostEntry[] {
  try {
    const v = JSON.parse(localStorage.getItem(HOSTS_KEY) ?? "[]");
    return Array.isArray(v) ? v : [];
  } catch {
    return [];
  }
}

export const hosts = ref<HostEntry[]>(load());
export const activeHostId = ref<string>(localStorage.getItem(ACTIVE_KEY) ?? "");

// ── 老版本迁移:单主机三键 → 主机列表 ──────────────────
// (老 APK 升级上来仍保留已登录主机,不用重新配置)
if (!hosts.value.length && getBase()) {
  const b = getBase();
  const entry: HostEntry = {
    id: newId(),
    name: hostLabel(b),
    base: b,
    addrs: [b],
    token: getToken() || undefined,
    user: getUser(),
    lastUsed: Date.now(),
  };
  hosts.value = [entry];
  activeHostId.value = entry.id;
  persist();
}

function persist(): void {
  localStorage.setItem(HOSTS_KEY, JSON.stringify(hosts.value));
  if (activeHostId.value) localStorage.setItem(ACTIVE_KEY, activeHostId.value);
  else localStorage.removeItem(ACTIVE_KEY);
}

function newId(): string {
  return typeof crypto !== "undefined" && "randomUUID" in crypto
    ? crypto.randomUUID()
    : `h-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

/** 地址 → 简短标签(去协议/去端口 8080 以外保留)。 */
export function hostLabel(base: string): string {
  return base.replace(/^https?:\/\//, "");
}

export function activeHost(): HostEntry | null {
  return hosts.value.find((h) => h.id === activeHostId.value) ?? null;
}

export function findByBase(base: string): HostEntry | null {
  const b = base.trim().replace(/\/+$/, "");
  return hosts.value.find((h) => h.base === b || h.addrs.includes(b)) ?? null;
}

/** 新增/合并一台主机(同地址视为同一台,合并候选地址)。返回该条目。 */
export function upsertHost(args: {
  base: string;
  addrs?: string[];
  name?: string;
  /** 主机 iroh NodeId(连接码带来):有它 probeEntry 才会优先起 iroh 隧道直连。 */
  nodeId?: string;
}): HostEntry {
  const base = args.base.trim().replace(/\/+$/, "");
  const extra = (args.addrs ?? []).map((a) => a.trim().replace(/\/+$/, ""));
  let h = findByBase(base) ?? extra.map(findByBase).find(Boolean) ?? null;
  if (h) {
    h.base = base;
    for (const a of [base, ...extra]) if (!h.addrs.includes(a)) h.addrs.push(a);
    if (args.name) h.name = args.name;
    if (args.nodeId) h.nodeId = args.nodeId;
  } else {
    h = {
      id: newId(),
      name: args.name?.trim() || hostLabel(base),
      base,
      addrs: [...new Set([base, ...extra])],
      nodeId: args.nodeId || undefined,
      lastUsed: Date.now(),
    };
    hosts.value = [h, ...hosts.value];
  }
  persist();
  return h;
}

export function removeHost(id: string): void {
  hosts.value = hosts.value.filter((h) => h.id !== id);
  if (activeHostId.value === id) {
    activeHostId.value = "";
    setBase("");
    setToken("");
    setUser(null);
    resetWs(); // 删的是当前主机 → 断开旧 WS,别对着已移除主机重连(codex 附)
  }
  persist();
}

export function renameHost(id: string, name: string): void {
  const h = hosts.value.find((x) => x.id === id);
  if (h && name.trim()) {
    h.name = name.trim();
    persist();
  }
}

/** 登录成功后把会话记到主机条目上(auth.ts 调用)。 */
export function saveSession(hostId: string, token: string, user: CollabUser | null): void {
  const h = hosts.value.find((x) => x.id === hostId);
  if (h) {
    h.token = token || undefined;
    h.user = user;
    h.lastUsed = Date.now();
    persist();
  }
}

/** 记住/清除某主机的账号密码(登录表单「记住密码」开关)。 */
export function saveCreds(hostId: string, username: string, password: string): void {
  const h = hosts.value.find((x) => x.id === hostId);
  if (h) {
    h.savedUsername = username || undefined;
    h.savedPassword = password || undefined;
    persist();
  }
}

/** 401/登出时清掉该主机的会话(主机条目保留,下次免填地址)。
 *  记住的账号密码不动 —— 它就是为了这种时刻静默重登用的。 */
export function dropSessionOf(hostId: string): void {
  const h = hosts.value.find((x) => x.id === hostId);
  if (h) {
    h.token = undefined;
    h.user = null;
    persist();
  }
}

/**
 * 激活某主机:把它的 base/token/user 写回 net 单例键并重连 WS。
 * 不做网络校验 —— 调用方自行决定先 probe 还是直接切。
 */
export function activateHost(h: HostEntry): void {
  activeHostId.value = h.id;
  h.lastUsed = Date.now();
  setBase(h.base);
  setToken(h.token ?? "");
  setUser(h.user ?? null);
  persist();
  resetWs();
}

/**
 * 探活一台主机的所有候选地址,返回第一个通的(并把它转正为 base)。
 * 全不通返回 null。
 */
export async function probeEntry(h: HostEntry, opts?: { tunnel?: boolean }): Promise<string | null> {
  const useTunnel = opts?.tunnel !== false; // 默认可起隧道;列表批量刷新传 false 避免争抢
  // 路线 B:有主机 NodeId 且原生隧道可用 → 优先起 iroh P2P 隧道直连桌面(打洞,失败自动
  // 走 relay 兜底),连通即用本地回环 base。失败再回落地址候选(局域网 IP / 云网关中继)。
  // 隧道是单端口独占,一次只能连一台 → 批量探活(probeAll)绝不能各自起隧道互相踢(codex #6)。
  if (useTunnel && h.nodeId && (await tunnelAvailable())) {
    const local = await startTunnel(h.nodeId);
    if (local) {
      // iroh 连接是异步建立的:relay 握手 + 打洞需数秒,失败会自动走 relay 中继兜底。
      // 起完隧道立刻探活时流还没通,必失败 —— 所以轮询 tunnelState 等它真「connected」
      // 再探,最多 ~18s。这样中继兜底(打洞不通时经 relay 转发)有时间生效。
      for (let i = 0; i < 18; i++) {
        const st = await tunnelState();
        if (st && st.state === "connected") {
          if (await probe(local)) return local;
        }
        if (st && st.state === "stopped" && st.last_error) break; // 彻底失败(连 relay 都连不上)
        await new Promise((r) => setTimeout(r, 1000));
      }
      if (await probe(local)) return local; // 收尾再探一次
    }
    await stopTunnel(); // 隧道起不来,收掉再走地址候选
  }
  // base 优先(上次连通的),其余候选跟上
  const order = [h.base, ...h.addrs.filter((a) => a !== h.base)];
  for (const a of order) {
    if (await probe(a)) {
      if (h.base !== a) {
        h.base = a;
        persist();
      }
      return a;
    }
  }
  return null;
}

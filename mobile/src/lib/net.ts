/**
 * 远程客户端 —— 手机壳与中控平台(polaris-server)的唯一通道。
 *
 * 一切请求走「用户配置的绝对 base」而非同源:
 *  - invoke(cmd,args)  → POST {base}/api/invoke   Bearer token       (≈75 命令:聊天/文件/工作)
 *  - listen(topic,cb)  → GET  {base}/ws?token=     WebSocket 事件分发  (聊天流 chat:stream 等)
 *  - upload(files)     → POST {base}/api/upload    multipart
 *  - fileUrl(path)     → GET  {base}/api/file?path=&token=
 *  - collab.*          → {base}/api/collab/*        账号/项目/任务
 *
 * 鉴权统一:collab 登录 JWT 一个 token 通吃 —— 既认 /api/invoke 又认 collab(server.rs:300)。
 * base + token + deviceId 持久化到 localStorage(Capacitor WebView 内持久),重启自动恢复会话。
 */

const BASE_KEY = "polaris.m.base";
const TOKEN_KEY = "polaris.m.token";
const USER_KEY = "polaris.m.user";
const DEVICE_KEY = "polaris.m.deviceId";

// ── 会话持久化 ────────────────────────────────────────

export function getBase(): string {
  return localStorage.getItem(BASE_KEY) ?? "";
}
export function setBase(b: string): void {
  const v = b.trim().replace(/\/+$/, "");
  if (v) localStorage.setItem(BASE_KEY, v);
  else localStorage.removeItem(BASE_KEY);
}
export function getToken(): string {
  return localStorage.getItem(TOKEN_KEY) ?? "";
}
export function setToken(t: string): void {
  if (t) localStorage.setItem(TOKEN_KEY, t);
  else localStorage.removeItem(TOKEN_KEY);
}
export function deviceId(): string {
  let id = localStorage.getItem(DEVICE_KEY);
  if (!id) {
    id =
      typeof crypto !== "undefined" && "randomUUID" in crypto
        ? crypto.randomUUID()
        : `dev-${Date.now()}-${Math.random().toString(36).slice(2, 10)}`;
    localStorage.setItem(DEVICE_KEY, id);
  }
  return id;
}

export interface CollabUser {
  id?: number;
  username: string;
  display_name?: string;
  displayName?: string;
  role: string;
}
export function getUser(): CollabUser | null {
  try {
    const raw = localStorage.getItem(USER_KEY);
    return raw ? (JSON.parse(raw) as CollabUser) : null;
  } catch {
    return null;
  }
}
export function setUser(u: CollabUser | null): void {
  if (u) localStorage.setItem(USER_KEY, JSON.stringify(u));
  else localStorage.removeItem(USER_KEY);
}

/** 退出:清空 token+user,保留 base 与 deviceId(下次同主机免填地址)。 */
export function clearSession(): void {
  setToken("");
  setUser(null);
}

// ── HTTP 封装 ─────────────────────────────────────────

function authHeaders(): Record<string, string> {
  const t = getToken();
  return t ? { Authorization: `Bearer ${t}` } : {};
}

export class NetError extends Error {
  status?: number;
  constructor(msg: string, status?: number) {
    super(msg);
    this.status = status;
  }
}

// 401 统一出口:token 过期/被吊销时由 auth 层注册回调把 UI 落回登录页,
// 而不是每个界面各自收到"登录已过期"却无路可走。
let onUnauthorized: (() => void) | null = null;
export function setOnUnauthorized(cb: () => void): void {
  onUnauthorized = cb;
}

async function rawFetch(path: string, init?: RequestInit): Promise<Response> {
  const base = getBase();
  if (!base) throw new NetError("尚未连接主机");
  try {
    return await fetch(base + path, init);
  } catch {
    throw new NetError("无法连接主机,请检查地址与网络");
  }
}

async function parse<T>(res: Response): Promise<T> {
  const text = await res.text();
  let data: unknown = null;
  let failed = false;
  if (text) {
    try {
      data = JSON.parse(text);
    } catch {
      failed = true;
    }
  }
  if (res.ok && failed) {
    throw new NetError("主机返回了非 JSON —— 请确认地址指向 Polaris 服务而非普通网页");
  }
  if (!res.ok) {
    // 已登录状态下收到 401 = token 失效,通知 auth 层落回登录页。
    // 未登录时(登录接口本身报 401)不触发,否则密码输错也会被"踢出"。
    if (res.status === 401 && getToken() && onUnauthorized) onUnauthorized();
    const msg =
      (data as { error?: string } | null)?.error ||
      (res.status === 401 ? "登录已过期,请重新登录" : `请求失败(HTTP ${res.status})`);
    throw new NetError(msg, res.status);
  }
  return data as T;
}

/** 核心命令分发:等价桌面 invoke()。cmd 见 server.rs dispatch(≈75)。 */
export async function invoke<T = unknown>(
  cmd: string,
  args?: Record<string, unknown>
): Promise<T> {
  const res = await rawFetch("/api/invoke", {
    method: "POST",
    headers: { "content-type": "application/json", ...authHeaders() },
    body: JSON.stringify({ cmd, args: args ?? {} }),
  });
  return parse<T>(res);
}

/** REST GET(collab 等) */
async function get<T>(path: string): Promise<T> {
  const res = await rawFetch(path, { headers: authHeaders() });
  return parse<T>(res);
}
/** REST POST(collab 等) */
async function post<T>(path: string, body?: unknown): Promise<T> {
  const res = await rawFetch(path, {
    method: "POST",
    headers: { "content-type": "application/json", ...authHeaders() },
    body: JSON.stringify(body ?? {}),
  });
  return parse<T>(res);
}

/** 上传文件 → 服务端临时路径(喂 chat_attach_files / kb_upload_files)。 */
export async function upload(
  files: File[] | FileList
): Promise<Array<{ name: string; path: string; size: number }>> {
  const fd = new FormData();
  for (const f of Array.from(files as ArrayLike<File>)) fd.append("files", f, f.name);
  const res = await rawFetch("/api/upload", {
    method: "POST",
    headers: { ...authHeaders() },
    body: fd,
  });
  const j = await parse<{ files?: Array<{ name: string; path: string; size: number }> }>(res);
  return j.files ?? [];
}

/** 受 token 保护的文件 URL(图片/预览;导航请求带不了头,token 走 query)。 */
export function fileUrl(path: string): string {
  const qs = new URLSearchParams({ path });
  const t = getToken();
  if (t) qs.set("token", t);
  return `${getBase()}/api/file?${qs.toString()}`;
}

// ── 探活 & 分享码 ─────────────────────────────────────

/** 探活某主机地址(免鉴权 /api/health,2.5s 超时)。 */
export async function probe(addr: string): Promise<boolean> {
  const base = addr.trim().replace(/\/+$/, "");
  try {
    const ctl = new AbortController();
    const t = setTimeout(() => ctl.abort(), 2500);
    const r = await fetch(base + "/api/health", { signal: ctl.signal });
    clearTimeout(t);
    return r.ok;
  } catch {
    return false;
  }
}

/** 分享码 PLRS1-<base64url{c,a}> → {code, addrs};非分享码返回 null。 */
export function parseShareCode(s: string): { code: string; addrs: string[] } | null {
  const m = s.trim();
  if (!m.startsWith("PLRS1-")) return null;
  try {
    const b64 = m.slice(6).replace(/-/g, "+").replace(/_/g, "/");
    const pad = b64 + "=".repeat((4 - (b64.length % 4)) % 4);
    const v = JSON.parse(atob(pad));
    if (typeof v.c !== "string" || !Array.isArray(v.a)) return null;
    return {
      code: v.c,
      addrs: v.a.filter((x: unknown): x is string => typeof x === "string"),
    };
  } catch {
    return null;
  }
}

/** 逐个探活分享码地址,返回第一个能通的。 */
export async function probeHost(addrs: string[]): Promise<string | null> {
  for (const a of addrs) {
    if (await probe(a)) return a.trim().replace(/\/+$/, "");
  }
  return null;
}

// ── WebSocket:按 topic 分发服务端 emit 的事件 ──────────

const wsListeners = new Map<string, Set<(p: unknown) => void>>();
let ws: WebSocket | null = null;
let reconnect: ReturnType<typeof setTimeout> | null = null;
// 指数退避:1.5s 起,每次失败翻倍,封顶 15s;连上即复位。
// 避免主机宕机时手机端每 1.5s 硬敲一次耗电耗网。
const BACKOFF_MIN = 1500;
const BACKOFF_MAX = 15000;
let backoff = BACKOFF_MIN;
const statusCbs = new Set<(connected: boolean) => void>();

export function onWsStatus(cb: (c: boolean) => void): () => void {
  statusCbs.add(cb);
  return () => statusCbs.delete(cb);
}
function emitStatus(c: boolean): void {
  for (const cb of statusCbs) cb(c);
}

function wsUrl(): string {
  const base = getBase();
  // http(s)://host → ws(s)://host/ws?token=
  const u = base.replace(/^http/, "ws");
  const t = getToken();
  return `${u}/ws${t ? `?token=${encodeURIComponent(t)}` : ""}`;
}

function ensureWs(): void {
  if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) return;
  if (!getBase()) return;
  try {
    ws = new WebSocket(wsUrl());
  } catch {
    ws = null;
    return;
  }
  ws.onopen = () => {
    backoff = BACKOFF_MIN;
    emitStatus(true);
  };
  ws.onmessage = (e) => {
    try {
      const { topic, payload } = JSON.parse(e.data as string) as {
        topic: string;
        payload: unknown;
      };
      const set = wsListeners.get(topic);
      if (set) for (const cb of set) cb(payload);
    } catch {
      /* 忽略坏帧 */
    }
  };
  ws.onclose = () => {
    ws = null;
    emitStatus(false);
    if (reconnect) clearTimeout(reconnect);
    if (wsListeners.size > 0) {
      reconnect = setTimeout(ensureWs, backoff);
      backoff = Math.min(backoff * 2, BACKOFF_MAX);
    }
  };
  ws.onerror = () => {
    try {
      ws?.close();
    } catch {
      /* ignore */
    }
    ws = null;
  };
}

/** 订阅服务端事件 topic;返回退订函数。 */
export function listen<T = unknown>(topic: string, cb: (payload: T) => void): () => void {
  let set = wsListeners.get(topic);
  if (!set) {
    set = new Set();
    wsListeners.set(topic, set);
  }
  const fn = cb as (p: unknown) => void;
  set.add(fn);
  ensureWs();
  return () => {
    set!.delete(fn);
    if (set!.size === 0) wsListeners.delete(topic);
  };
}

/** 会话变化(登录/切主机)后重连 WS。 */
export function resetWs(): void {
  if (reconnect) {
    clearTimeout(reconnect);
    reconnect = null;
  }
  backoff = BACKOFF_MIN;
  try {
    ws?.close();
  } catch {
    /* ignore */
  }
  ws = null;
  if (wsListeners.size > 0) ensureWs();
}

// 安卓 WebView 切后台会掐 WS,且挂起期间 close/定时器可能被冻结 →
// 回前台 / 网络恢复时立刻主动补一次重连,不等退避定时器。
if (typeof document !== "undefined") {
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible" && wsListeners.size > 0) {
      backoff = BACKOFF_MIN;
      ensureWs();
    }
  });
}
if (typeof window !== "undefined") {
  window.addEventListener("online", () => {
    if (wsListeners.size > 0) {
      backoff = BACKOFF_MIN;
      ensureWs();
    }
  });
}

// ── collab REST 子集(手机端够用的部分) ────────────────

export interface AuthResult {
  user: CollabUser;
  token: string;
}
export interface CollabProject {
  id: number;
  name: string;
  repo: string;
  team_id?: number | null;
  open_count?: number;
  review_count?: number;
  archived?: boolean;
}
export type TaskState =
  | "pending"
  | "in_progress"
  | "review"
  | "merged"
  | "archived"
  | "cancelled";
export interface TaskCard {
  id: number;
  project_id: number;
  title: string;
  body: string;
  scope: string;
  criteria: string;
  assignee: number | null;
  state: TaskState;
  round: number;
  branch: string;
  created_at: number;
  updated_at: number;
}

export const collab = {
  health: () => get<{ ok?: boolean }>("/api/health"),
  bootstrap: (a: { username: string; password: string; displayName: string; hostSelf?: boolean }) =>
    post<AuthResult>("/api/collab/bootstrap", { ...a, deviceId: deviceId() }),
  login: (a: { username: string; password: string }) =>
    post<AuthResult>("/api/collab/login", { ...a, deviceId: deviceId() }),
  signup: (a: { username: string; password: string; displayName: string }) =>
    post<AuthResult>("/api/collab/signup", { ...a, deviceId: deviceId() }),
  redeem: (a: {
    code: string;
    username: string;
    password: string;
    displayName: string;
    deviceName: string;
    nodeId: string;
  }) => post<AuthResult>("/api/collab/redeem", a),
  logout: () => post<void>("/api/collab/logout"),
  me: () => get<{ username: string; role: string }>("/api/collab/me"),
  listProjects: () => get<CollabProject[]>("/api/collab/projects"),
  listTasks: (projectId: number) =>
    get<TaskCard[]>(`/api/collab/tasks?projectId=${String(projectId)}`),
  claimTask: (taskId: number) => post<void>("/api/collab/task/claim", { taskId }),
  submitTask: (taskId: number) => post<void>("/api/collab/task/submit", { taskId }),
};

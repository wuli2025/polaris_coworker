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

/**
 * 取文件内容,**token 走 Authorization 头、不进 URL**。
 *
 * 用于 HTML/SVG/文本预览:
 *  ① 服务端对 html/svg/js 会加 `Content-Disposition: attachment`(防止带 token 的
 *     URL 被当页面加载、页内脚本读走 owner 令牌),iframe 直接 src= 必白屏 —— fetch 不受该头影响;
 *  ② URL 里不出现令牌,拿到文本后塞进 sandbox iframe 的 srcdoc,脚本跑在 opaque origin,
 *     够不着主机同源接口,比原来的 `?token=` 直载更安全。
 * 调用方自己判 res.ok(把 401/403/404 的中文原因显示出来,别静默白屏)。
 */
export async function fileFetch(path: string): Promise<Response> {
  const qs = new URLSearchParams({ path });
  return rawFetch(`/api/file?${qs.toString()}`, { headers: authHeaders() });
}

/** 受 token 保护的文件 URL(图片/音视频等元素加载;元素请求带不了头,token 只能走 query)。 */
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
export function parseShareCode(
  s: string
): { code: string; addrs: string[]; authority?: string; kid?: string } | null {
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
      // u/k:主机的账号由云端账号中心统管时带上,收码人据此知道去哪儿注册/登录。
      authority: typeof v.u === "string" && v.u ? v.u : undefined,
      kid: typeof v.k === "string" && v.k ? v.k : undefined,
    };
  } catch {
    return null;
  }
}

/** 连接码 PLRK1-<base64url(json{t:token, a:[addrs]})> → {token, addrs};非该格式返回 null。
 *  同账号自己的设备用:主机端「互联」把地址+owner令牌打包成一串,手机粘上即以 owner
 *  完整权限直接连上,不走登录/邀请(令牌本身即凭据)。区别于 PLRS1(邀请别人,走 redeem)。 */
export function parseConnectCode(
  s: string
): { token: string; addrs: string[]; nodeId?: string; authority?: string } | null {
  const m = s.trim();
  if (!m.startsWith("PLRK1-")) return null;
  try {
    const b64 = m.slice(6).replace(/-/g, "+").replace(/_/g, "/");
    const pad = b64 + "=".repeat((4 - (b64.length % 4)) % 4);
    const v = JSON.parse(atob(pad));
    if (typeof v.t !== "string" || !Array.isArray(v.a)) return null;
    return {
      token: v.t,
      addrs: v.a.filter((x: unknown): x is string => typeof x === "string"),
      // n = 主机 iroh NodeId:有它 + 原生隧道可用 → probeEntry 优先 iroh 打洞 P2P 直连。
      nodeId: typeof v.n === "string" && v.n ? v.n : undefined,
      // u = 该主机的云端账号中心(令牌进门用不着,换账号登录时要)。
      authority: typeof v.u === "string" && v.u ? v.u : undefined,
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

/** 主机的账号体系自述:决定登录该往主机打还是往云端账号中心打。 */
export interface AccountInfo {
  mode: "authority" | "delegated" | "local";
  authorityUrl?: string;
  trusted?: boolean;
  kid?: string;
  emailRequired: boolean;
}

/**
 * 打**云端账号中心**的绝对地址(不是当前主机)。
 * 注册/改密/换身份断言都发生在云端;主机没有密码可验,也不该看见密码。
 */
async function authorityFetch<T>(
  authorityUrl: string,
  path: string,
  body?: unknown
): Promise<T> {
  const base = authorityUrl.trim().replace(/\/+$/, "");
  if (!base) throw new NetError("还没有配置云端账号中心地址");
  let res: Response;
  try {
    res = await fetch(base + path, {
      method: body === undefined ? "GET" : "POST",
      headers: body === undefined ? {} : { "content-type": "application/json" },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
  } catch {
    throw new NetError("连不上云端账号中心,请检查网络");
  }
  const text = await res.text();
  let data: unknown = null;
  try {
    data = text ? JSON.parse(text) : null;
  } catch {
    throw new NetError("云端账号中心返回了非 JSON —— 请确认地址填对了");
  }
  if (!res.ok) {
    throw new NetError(
      (data as { error?: string } | null)?.error ??
        `账号中心请求失败(HTTP ${res.status})`,
      res.status
    );
  }
  return data as T;
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

  // ── 云端账号中心(账号权威)──
  /** 本机主机的账号体系自述(公开,免登录) */
  accountInfo: () => get<AccountInfo>("/api/account/info"),
  /** 拿云端断言换**当前主机**的会话:主机纯本地验签,断网也认 */
  loginAssertion: (assertion: string) =>
    post<AuthResult>("/api/collab/login_assertion", {
      assertion,
      deviceId: deviceId(),
    }),
  /** 云端登录:用户名**或邮箱** + 密码 → 身份断言 */
  authorityLogin: (url: string, a: { username: string; password: string }) =>
    authorityFetch<{ assertion: string; uid: string }>(
      url,
      "/api/account/login",
      a
    ),
  /** 云端注册:必须带邮箱验证码 */
  authoritySignup: (
    url: string,
    a: {
      email: string;
      code: string;
      username: string;
      password: string;
      displayName: string;
    }
  ) =>
    authorityFetch<{ assertion: string; uid: string }>(
      url,
      "/api/account/signup",
      a
    ),
  /** 云端发验证码(注册/找回共用) */
  authoritySendCode: (url: string, email: string, purpose: "signup" | "reset") =>
    authorityFetch<void>(url, "/api/collab/email/send_code", { email, purpose }),
  /** 云端账号 + 本机邀请码 → 成为这台主机的成员 */
  joinWithTicket: (a: { assertion: string; code: string; deviceName: string }) =>
    post<AuthResult>("/api/collab/join", { ...a, nodeId: deviceId() }),
  /** 云端改密:改完所有主机同时生效 */
  authorityReset: (
    url: string,
    a: { email: string; code: string; newPassword: string }
  ) =>
    authorityFetch<{ ok: boolean; username?: string }>(
      url,
      "/api/account/reset",
      a
    ),
  listProjects: () => get<CollabProject[]>("/api/collab/projects"),
  listTasks: (projectId: number) =>
    get<TaskCard[]>(`/api/collab/tasks?projectId=${String(projectId)}`),
  claimTask: (taskId: number) => post<void>("/api/collab/task/claim", { taskId }),
  submitTask: (taskId: number) => post<void>("/api/collab/task/submit", { taskId }),
};

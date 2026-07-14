import { ref, computed } from "vue";
import {
  collab,
  getBase,
  getToken,
  getUser,
  setToken,
  setUser,
  resetWs,
  deviceId,
  setOnUnauthorized,
  type CollabUser,
} from "./net";
import { activeHostId, activeHost, saveSession, dropSessionOf, saveCreds, type HostEntry } from "./hosts";

export const user = ref<CollabUser | null>(getUser());
export const token = ref(getToken());

export const authed = computed(() => !!token.value && !!user.value);
export const isOwner = computed(() => user.value?.role === "owner");
/** 当前主机地址(展示用)。 */
export const base = computed(() => activeHost()?.base ?? getBase());
/** 当前主机名(标题栏用)。 */
export const hostName = computed(() => activeHost()?.name ?? "");

function persist(t: string, u: CollabUser | null): void {
  setToken(t);
  setUser(u);
  token.value = t;
  user.value = u;
  saveSession(activeHostId.value, t, u); // 会话记到主机条目,切回来免登录
  resetWs();
}

/** 切主机(hosts.activateHost)后同步 auth 层的响应式状态。 */
export function syncFromActiveHost(): void {
  token.value = getToken();
  user.value = getUser();
}

export async function login(username: string, password: string, remember = false): Promise<void> {
  const r = await collab.login({ username, password });
  persist(r.token, r.user);
  // 记住密码:勾了存,没勾清 —— 状态跟着最近一次登录走,不残留旧密码。
  saveCreds(activeHostId.value, remember ? username : "", remember ? password : "");
}

/**
 * 无感续会话(自动连接/401 重试共用):
 *  ① 有 token → 验一下还活着;② 死了但记住过密码 → 静默重登。
 * 都不行返回 false,调用方再决定是否打扰用户。
 */
export async function tryResume(h: HostEntry): Promise<boolean> {
  if (h.token) {
    try {
      await collab.me();
      return true;
    } catch {
      /* token 失效 → 试密码 */
    }
  }
  if (h.savedUsername && h.savedPassword) {
    try {
      const r = await collab.login({ username: h.savedUsername, password: h.savedPassword });
      persist(r.token, r.user);
      return true;
    } catch {
      /* 密码也不对(被改过)→ 交回登录页 */
    }
  }
  return false;
}

export async function signup(
  username: string,
  password: string,
  displayName: string
): Promise<void> {
  const r = await collab.signup({ username, password, displayName });
  persist(r.token, r.user);
}

export async function bootstrap(
  username: string,
  password: string,
  displayName: string
): Promise<void> {
  const r = await collab.bootstrap({ username, password, displayName });
  persist(r.token, r.user);
}

/** PLRK1 连接码:直接用 owner 令牌连上(同账号自己的设备)。设令牌 → 验证 → 建会话。
 *  不走登录/邀请:令牌本身就是凭据,验证过 /api/collab/me 拿到身份即完成。 */
export async function connectWithToken(t: string): Promise<CollabUser> {
  setToken(t);
  token.value = t;
  const me = await collab.me(); // 验证令牌有效并取 {username, role};无效会抛错(调用方兜错)
  const u: CollabUser = { username: me.username, role: me.role };
  persist(t, u);
  return u;
}

export async function redeem(args: {
  code: string;
  username: string;
  password: string;
  displayName: string;
}): Promise<void> {
  const r = await collab.redeem({
    ...args,
    deviceName: "Android",
    nodeId: deviceId(),
  });
  persist(r.token, r.user);
}

/** 本地登出(不打后端):401 被踢 / 主动退出共用的收尾。历史对话保留在本地。 */
function dropSession(): void {
  setToken("");
  setUser(null);
  token.value = "";
  user.value = null;
  dropSessionOf(activeHostId.value);
  resetWs();
  // 动态 import 斩断 auth ↔ chat 静态环(chat → net,auth → chat 会成环感知差;
  // 且登出是冷路径,异步清理无碍)。
  import("./chat").then((m) => m.resetChat()).catch(() => {});
}

// token 过期/被吊销 → 先用记住的密码静默重登一次(不打断正在用的人),
// 重登也失败才落回主机选择页。relogging 防 401 风暴下的重入循环。
let relogging = false;
setOnUnauthorized(() => {
  const h = activeHost();
  if (!relogging && h?.savedUsername && h.savedPassword) {
    relogging = true;
    const forHost = activeHostId.value; // 捕获发起时的主机
    collab
      .login({ username: h.savedUsername, password: h.savedPassword })
      .then((r) => {
        // 重登期间用户可能已切主机:结果只在仍是原主机时落地,否则丢弃(codex #3)。
        if (activeHostId.value === forHost) persist(r.token, r.user);
      })
      .catch(() => {
        if (activeHostId.value === forHost) dropSession();
      })
      .finally(() => {
        relogging = false;
      });
    return;
  }
  dropSession();
});

export async function logout(): Promise<void> {
  // 主动退出:清掉记住的账号密码,否则 boot 会用它自动登回来(codex #4)。
  saveCreds(activeHostId.value, "", "");
  try {
    await collab.logout();
  } catch {
    /* 忽略 */
  }
  dropSession();
}

export function displayName(u: CollabUser | null): string {
  return u?.displayName || u?.display_name || u?.username || "我";
}

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
  type AccountInfo,
  type AuthResult,
  type CollabUser,
} from "./net";
import { activeHostId, activeHost, saveSession, dropSessionOf, saveCreds, type HostEntry } from "./hosts";
import { resetChat } from "./chat";

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

/** 当前主机的账号体系(切主机后失效,故每次连上重新问)。 */
export const accountInfo = ref<AccountInfo | null>(null);
/** 账号由云端账号中心统管 —— 一个账号在所有主机上都能登。 */
export const federated = computed(() => accountInfo.value?.mode === "delegated");
export const authorityUrl = computed(() => accountInfo.value?.authorityUrl ?? "");

/** 问一次当前主机「账号该在哪儿登」。老主机没这个端点 → 按本机账号处理。 */
export async function loadAccountInfo(): Promise<AccountInfo | null> {
  try {
    accountInfo.value = await collab.accountInfo();
  } catch {
    accountInfo.value = { mode: "local", emailRequired: false };
  }
  return accountInfo.value;
}

/**
 * 只跑网络、**不落任何状态**的登录。账号云端统管时:先到账号中心验密码换身份断言,
 * 再拿断言进主机换本机会话 —— 密码直连云端,不经主机;主机只做本地验签,
 * 自己断网也放得了人进来。
 *
 * 之所以把「拿到会话」和「写进状态」拆开:401 静默重登是异步的,期间用户可能已经切了主机,
 * 那时候再写就会把 A 主机的会话按到 B 主机头上。调用方必须在**结果到手后**复核主机没变再落地。
 */
async function fetchSession(
  username: string,
  password: string
): Promise<AuthResult> {
  if (accountInfo.value === null) await loadAccountInfo();
  if (federated.value && authorityUrl.value) {
    const { assertion } = await collab.authorityLogin(authorityUrl.value, {
      username,
      password,
    });
    return collab.loginAssertion(assertion);
  }
  return collab.login({ username, password });
}

export async function login(username: string, password: string, remember = false): Promise<void> {
  const r = await fetchSession(username, password);
  persist(r.token, r.user);
  // 记住密码:勾了存,没勾清 —— 状态跟着最近一次登录走,不残留旧密码。
  saveCreds(activeHostId.value, remember ? username : "", remember ? password : "");
}

/** 云端注册(必须绑邮箱),成功即用返回的断言进当前主机。 */
export async function signupViaAuthority(args: {
  email: string;
  code: string;
  username: string;
  password: string;
  displayName: string;
}): Promise<void> {
  if (accountInfo.value === null) await loadAccountInfo();
  if (!authorityUrl.value) throw new Error("本主机没有配置云端账号中心");
  const { assertion } = await collab.authoritySignup(authorityUrl.value, args);
  const r = await collab.loginAssertion(assertion);
  persist(r.token, r.user);
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
      // 走 fetchSession(带联邦分支)——账号云端统管时得先换身份断言,
      // 直打主机密码接口对联邦账号永远失败(密码根本不在主机上)。
      const r = await fetchSession(h.savedUsername, h.savedPassword);
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

/**
 * 邀请码入伙。两条路由自动选:
 *  · 联邦主机 → 云端验密码换断言,再拿「断言 + 邀请码」入伙(身份来自云端,准入来自邀请码)。
 *  · 本机账号主机 → 老的 redeem(就地建号)。
 * 主机没登记过账号中心时,用分享码里带来的地址兜底(收码人可能是全新设备)。
 */
export async function redeem(args: {
  code: string;
  username: string;
  password: string;
  displayName: string;
  /** 分享码里带的账号中心地址(主机自述缺失时的兜底) */
  authorityHint?: string;
}): Promise<void> {
  if (accountInfo.value === null) await loadAccountInfo();
  const url = authorityUrl.value || args.authorityHint || "";
  if (federated.value && url) {
    const { assertion } = await collab.authorityLogin(url, {
      username: args.username,
      password: args.password,
    });
    const r = await collab.joinWithTicket({
      assertion,
      code: args.code,
      deviceName: "Android",
    });
    persist(r.token, r.user);
    return;
  }
  const r = await collab.redeem({
    code: args.code,
    username: args.username,
    password: args.password,
    displayName: args.displayName,
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
  resetChat();
}

// token 过期/被吊销 → 先用记住的密码静默重登一次(不打断正在用的人),
// 重登也失败才落回主机选择页。relogging 防 401 风暴下的重入循环。
let relogging = false;
setOnUnauthorized(() => {
  const h = activeHost();
  if (!relogging && h?.savedUsername && h.savedPassword) {
    relogging = true;
    const forHost = activeHostId.value; // 捕获发起时的主机
    // 必须走 fetchSession(带联邦分支),不能直接打主机的密码接口 ——
    // 联邦账号的密码根本不在主机上,直打必然失败。
    fetchSession(h.savedUsername, h.savedPassword)
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

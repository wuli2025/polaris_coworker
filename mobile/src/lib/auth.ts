import { ref, computed } from "vue";
import {
  collab,
  getBase,
  getToken,
  getUser,
  setBase,
  setToken,
  setUser,
  clearSession,
  resetWs,
  deviceId,
  setOnUnauthorized,
  type CollabUser,
} from "./net";

export const base = ref(getBase());
export const user = ref<CollabUser | null>(getUser());
export const token = ref(getToken());

export const authed = computed(() => !!token.value && !!user.value);
export const isOwner = computed(() => user.value?.role === "owner");

function persist(b: string, t: string, u: CollabUser | null): void {
  setBase(b);
  setToken(t);
  setUser(u);
  base.value = b;
  token.value = t;
  user.value = u;
  resetWs();
}

/** 连接主机(填地址,尚未登录):校验地址可达并落盘。 */
export async function connectHost(addr: string): Promise<void> {
  const b = addr.trim().replace(/\/+$/, "");
  setBase(b);
  base.value = b;
  await collab.health(); // 探活;失败抛错由调用方展示
}

export async function login(username: string, password: string): Promise<void> {
  const r = await collab.login({ username, password });
  persist(base.value, r.token, r.user);
}

export async function signup(
  username: string,
  password: string,
  displayName: string
): Promise<void> {
  const r = await collab.signup({ username, password, displayName });
  persist(base.value, r.token, r.user);
}

export async function bootstrap(
  username: string,
  password: string,
  displayName: string
): Promise<void> {
  const r = await collab.bootstrap({ username, password, displayName });
  persist(base.value, r.token, r.user);
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
  persist(base.value, r.token, r.user);
}

/** 本地登出(不打后端):401 被踢 / 主动退出共用的收尾。 */
function dropSession(): void {
  clearSession();
  token.value = "";
  user.value = null;
  resetWs();
  // 动态 import 斩断 auth ↔ chat 静态环(chat → net,auth → chat 会成环感知差;
  // 且登出是冷路径,异步清理无碍)。
  import("./chat").then((m) => m.resetChat()).catch(() => {});
}

// token 过期/被吊销 → 任意请求 401 时自动落回登录页
setOnUnauthorized(dropSession);

export async function logout(): Promise<void> {
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

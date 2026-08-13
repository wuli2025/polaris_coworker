/**
 * stores/account.ts —— 全应用**唯一**的身份来源。
 *
 * 在此之前,「我是谁」散在三个地方各说各话:协作 store 的 `user`(本机会话)、互联页的
 * `mesh.uid`(云端账号)、以及页面上那张写着 `owner · 全权` 的小卡片(本机角色)。同一个人
 * 在同一屏上被表示成三份,于是用户看到的就是「怎么还要登两个账号,还分什么云端本地」。
 *
 * 这个 store 把它们合成一份:
 *  · **谁**   = 邮箱(唯一登录凭据)+ uid(全局身份,跨主机认人只认它)
 *  · **在这台机器上是什么角色** = 本机会话给的 role
 *  · **有几台设备** = 设备台账
 *
 * 它不自己发请求登录 —— 登录只有一个入口(`features/auth/EmailLogin.vue`)。这里只负责
 * 把已经发生的事实汇总起来,给各个页面读同一份。
 */
import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { invoke, isTauri } from "../tauri";
import { useCollabStore } from "../features/collab/stores/collab";

/** 设备台账里的一台设备(与 src-tauri/src/mesh.rs 的 mesh_devices 输出一一对应)。 */
export interface AccountDevice {
  nodeId: string;
  name: string;
  os: string;
  ver: string;
  online: boolean;
  isSelf: boolean;
  revoked: boolean;
  localRevoked: boolean;
  drive: string;
  inCooldown: boolean;
}

export const useAccountStore = defineStore("account", () => {
  const collab = useCollabStore();

  /** 云端全局账号 id。空 = 这台机器还没登录过。 */
  const uid = ref("");
  /** 登录用的邮箱 —— 用户心里的「我的账号」就是它。 */
  const email = ref("");
  const devices = ref<AccountDevice[]>([]);
  /** 云端账号中心暂时联系不上时的原因(不是致命错,台账退回本机数据画)。 */
  const cloudError = ref("");
  const loading = ref(false);

  /** 已登录 = 有本机会话。桌面上它与「已入网」是同一件事(一步登录一起做完的)。 */
  const authed = computed(() => collab.authed);
  /** 这台机器上的角色。自己的设备恒为 owner;别人主机上的角色由那台机器说了算。 */
  const role = computed(() => collab.user?.role ?? "");
  const isOwner = computed(() => role.value === "owner");
  /** 展示名:优先邮箱(那才是用户认得的东西),没有再退回昵称/用户名。 */
  const label = computed(
    () =>
      email.value ||
      collab.user?.display_name ||
      collab.user?.displayName ||
      collab.user?.username ||
      ""
  );
  const onlineCount = computed(() => devices.value.filter((d) => d.online).length);
  /** 有设备在冷静期里等着被信任 —— UI 要冒个红点提醒用户去看一眼。 */
  const pendingTrust = computed(() => devices.value.filter((d) => d.inCooldown && !d.isSelf));

  /** 刷新身份 + 设备台账。桌面走一条 mesh_devices 命令拿全;网页只有身份没有设备网。 */
  async function refresh() {
    if (loading.value) return;
    loading.value = true;
    try {
      if (isTauri) {
        const r = (await invoke("mesh_devices")) as {
          uid?: string;
          cloudError?: string;
          devices?: AccountDevice[];
        };
        uid.value = r.uid ?? "";
        cloudError.value = r.cloudError ?? "";
        devices.value = r.devices ?? [];
      }
      // 邮箱只有账号中心知道,本机会话里带的是用户名 —— 登录时由 EmailLogin 存下来。
      if (!email.value) email.value = localStorage.getItem(EMAIL_KEY) ?? "";
    } catch {
      /* 老版本 / 非主机构建:身份卡退化成只显示会话身份 */
    } finally {
      loading.value = false;
    }
  }

  /** 登录成功时记下邮箱(刷新后仍要显示「你是谁」,而会话里并没有邮箱)。 */
  function rememberEmail(e: string) {
    email.value = e.trim();
    try {
      localStorage.setItem(EMAIL_KEY, email.value);
    } catch {
      /* storage 不可用 */
    }
  }

  /**
   * 退出登录 —— **一处退干净**。
   *
   * 此前「登出」只清本机会话,设备密钥还留着后台继续连着别人的机器;用户以为退了,
   * 其实盘还挂着。这里两件一起做:退设备网(拆盘、断隧道)+ 清本机会话。
   */
  async function logout() {
    if (isTauri) {
      try {
        await invoke("mesh_leave");
      } catch {
        /* 没入过网:忽略 */
      }
    }
    await collab.logout();
    uid.value = "";
    devices.value = [];
    email.value = "";
    try {
      localStorage.removeItem(EMAIL_KEY);
    } catch {
      /* storage 不可用 */
    }
  }

  return {
    uid,
    email,
    devices,
    cloudError,
    loading,
    authed,
    role,
    isOwner,
    label,
    onlineCount,
    pendingTrust,
    refresh,
    rememberEmail,
    logout,
  };
});

const EMAIL_KEY = "polaris.account.email.v1";

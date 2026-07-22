<script setup lang="ts">
/**
 * 主机与登录 —— App 第一屏,交互按「零打扰」设计:
 *
 *  · 回头客(有已存主机):进屏自动探活上次主机 → token 活着/记住过密码 → 静默续登,
 *    直接落进对话 —— 什么都不用点。失败才显示主机列表。
 *  · 新用户(首装):两步向导 —— ①填账号密码(记住密码默认开) ②粘授权码(连接码/地址,
 *    只填这一次,以后都记住) → 连接+登录一气呵成。
 *  · 老主机会话失效:登录表单预填记住的账号密码,一键进。
 */
import { onMounted, ref } from "vue";
import {
  hosts,
  activeHostId,
  upsertHost,
  removeHost,
  activateHost,
  probeEntry,
  hostLabel,
  saveCreds,
  type HostEntry,
} from "../lib/hosts";
import {
  authed,
  login,
  signup,
  bootstrap,
  redeem,
  connectWithToken,
  tryResume,
  syncFromActiveHost,
  displayName,
} from "../lib/auth";
import { parseShareCode, parseConnectCode, setBase } from "../lib/net";
import { initChat, reloadForHost } from "../lib/chat";
import { tunnelState } from "../lib/tunnel";
import { go } from "../lib/nav";
import { toast, toastErr } from "../lib/toast";

type View = "boot" | "list" | "setup" | "login";
// 有已存主机 → 先试自动连接(boot);全新安装 → 直接进两步向导。
const view = ref<View>(hosts.value.length ? "boot" : "setup");
const bootHint = ref("");
const busy = ref(false);
const busyHostId = ref("");

// 探活状态:hostId → "on" | "off" | "probing"
const status = ref<Record<string, string>>({});

// ── 首装向导(两步:①账号 ②授权码) ──
const step = ref<1 | 2>(1);
const username = ref("");
const password = ref("");
const nick = ref("");
const remember = ref(true); // 记住密码默认开:私人设备,少打扰
type Mode = "login" | "signup";
const mode = ref<Mode>("login");
// 步骤②:授权码 / 地址
const addr = ref("");
const addName = ref("");

// ── 老主机重登(pendingHost) ──
const pendingHost = ref<HostEntry | null>(null);

/** 进入主界面(登录/续会话成功后共用的收尾)。 */
function enter(sameHost = false) {
  if (!sameHost) reloadForHost();
  initChat();
  go("chat");
}

/** 回头客自动连接:上次主机 → 探活 → 无感续登 → 直接进。 */
async function autoConnect() {
  const last =
    hosts.value.find((h) => h.id === activeHostId.value) ??
    [...hosts.value].sort((a, b) => b.lastUsed - a.lastUsed)[0];
  if (!last) {
    view.value = "setup";
    return;
  }
  bootHint.value = `正在连接「${last.name}」…`;
  try {
    const ok = await probeEntry(last);
    if (!ok) throw new Error("offline");
    const sameHost = last.id === activeHostId.value && authed.value;
    activateHost(last);
    setBase(ok); // probeEntry 探到的实际通道(可能是 iroh 回环口);activateHost 会写回 h.base,须覆盖(codex #5)
    syncFromActiveHost();
    bootHint.value = "验证身份…";
    if (await tryResume(last)) {
      syncFromActiveHost();
      enter(sameHost);
      return;
    }
    // 主机通但身份进不去 → 打开登录表单(预填记住的账号)
    openLogin(last);
  } catch {
    // 主机不在线 → 回列表让用户自己挑/换
    view.value = "list";
    probeAll();
  }
}

async function probeAll() {
  // 列表批量探活只查 HTTP 地址,不起 iroh 隧道 —— 隧道单端口独占,并发会互相踢(codex #6)。
  await Promise.all(
    hosts.value.map(async (h) => {
      status.value = { ...status.value, [h.id]: "probing" };
      const ok = await probeEntry(h, { tunnel: false });
      status.value = { ...status.value, [h.id]: ok ? "on" : "off" };
    })
  );
}

onMounted(() => {
  if (view.value === "boot") autoConnect();
});

function openLogin(h: HostEntry) {
  pendingHost.value = h;
  username.value = h.savedUsername ?? h.user?.username ?? "";
  password.value = h.savedPassword ?? "";
  remember.value = !!h.savedPassword || true;
  mode.value = "login";
  view.value = "login";
}

/** 点主机卡:探活 → 激活 → 无感续登直接进,不行才登录表单。 */
async function tapHost(h: HostEntry) {
  if (busy.value) return;
  busy.value = true;
  busyHostId.value = h.id;
  try {
    status.value = { ...status.value, [h.id]: "probing" };
    const ok = await probeEntry(h);
    status.value = { ...status.value, [h.id]: ok ? "on" : "off" };
    if (!ok) {
      toast("主机不在线:所有地址都连不上", "error");
      return;
    }
    const sameHost = h.id === activeHostId.value && authed.value;
    activateHost(h);
    setBase(ok); // 同 boot:用实际连通通道覆盖 activateHost 写回的 h.base(codex #5)
    syncFromActiveHost();
    if (await tryResume(h)) {
      syncFromActiveHost();
      enter(sameHost);
      return;
    }
    openLogin(h);
  } finally {
    busy.value = false;
    busyHostId.value = "";
  }
}

/** 向导步①:校验账号字段 → 进步② */
function nextStep() {
  if (!username.value.trim()) return toast("先填用户名", "error");
  if (!password.value) return toast("先填密码", "error");
  step.value = 2;
}

/** 向导步②:授权码/地址 → 连接 → 用步①的账号登录。授权码只填这一次。 */
async function doSetup() {
  const raw = addr.value.trim();
  if (!raw) return toast("粘贴授权码(连接码)或填主机地址", "error");
  busy.value = true;
  try {
    const u = username.value.trim();
    const n = nick.value.trim() || u;

    // ① 解析授权码:PLRK1(自己的设备)带地址+NodeId+owner令牌;PLRS1(受邀)带地址+邀请码。
    const conn = parseConnectCode(raw);
    const share = conn ? null : parseShareCode(raw);
    let h: HostEntry;
    if (conn) {
      h = upsertHost({
        base: conn.addrs[0] || "http://127.0.0.1:0",
        addrs: conn.addrs.length ? conn.addrs : [],
        name: addName.value.trim() || undefined,
        nodeId: conn.nodeId,
      });
    } else if (share) {
      h = upsertHost({ base: share.addrs[0] ?? "", addrs: share.addrs, name: addName.value.trim() || undefined });
    } else {
      let base = /^https?:\/\//.test(raw) ? raw : `http://${raw}`;
      base = base.replace(/\/+$/, "");
      h = upsertHost({ base, name: addName.value.trim() || undefined });
    }

    // ② 连通:iroh 优先(有 NodeId),失败回落地址候选。
    activateHost(h);
    const live = await probeEntry(h);
    if (!live) {
      const st = await tunnelState();
      const why = st?.last_error || "打洞/中继/地址都没连上";
      throw new Error(`连不上:${why}${st ? `(iroh:${st.state})` : ""}`);
    }
    setBase(live);

    // ③ 身份:优先步①的账号密码(账号密码为主);受邀码走 redeem;
    //    登录报「无此账号」且是注册模式 → signup/bootstrap。
    if (share?.code) {
      // 分享码可能带着主机的账号中心地址(联邦主机):全新设备还没问过主机自述时用它兜底。
      await redeem({
        code: share.code,
        username: u,
        password: password.value,
        displayName: n,
        authorityHint: share.authority,
      });
      saveCreds(activeHostId.value, remember.value ? u : "", remember.value ? password.value : ""); // codex #10
    } else if (mode.value === "signup") {
      // signup/bootstrap 已返回 token 并持久化,不再重复 login(会多造 session,二次失败反把成功当失败,codex #11)。
      try {
        await signup(u, password.value, n);
      } catch (e) {
        const msg = (e as Error).message || "";
        if (/初始化|bootstrap|owner|无账号|未初始化/.test(msg)) {
          await bootstrap(u, password.value, n);
        } else throw e;
      }
      saveCreds(activeHostId.value, remember.value ? u : "", remember.value ? password.value : "");
    } else {
      try {
        await login(u, password.value, remember.value);
      } catch (e) {
        // 账号密码进不去,但 PLRK1 自带 owner 令牌 → 兜底直连(同账号自己的设备)。
        // 令牌进入不代表密码对 → 明确清掉记住的密码,不假装已记住(codex #10)。
        if (conn?.token) {
          await connectWithToken(conn.token);
          saveCreds(activeHostId.value, "", "");
          toast("账号密码没对上,已用连接码令牌进入(未记住密码)", "ok");
        } else throw e;
      }
    }

    addr.value = "";
    addName.value = "";
    toast("已连接", "ok");
    enter();
    view.value = "list";
    step.value = 1;
  } catch (e) {
    toastErr(e);
  } finally {
    busy.value = false;
  }
}

/** 老主机重登(登录表单)。 */
async function doLogin() {
  busy.value = true;
  try {
    await login(username.value.trim(), password.value, remember.value);
    enter();
    view.value = "list";
  } catch (e) {
    toastErr(e);
  } finally {
    busy.value = false;
  }
}

function delHost(h: HostEntry, e: Event) {
  e.stopPropagation();
  if (confirm(`移除主机「${h.name}」?本机保存的该主机历史对话不受影响。`)) {
    removeHost(h.id);
  }
}

function stateLabel(h: HostEntry): string {
  const s = status.value[h.id];
  if (s === "probing") return "探测中…";
  if (s === "on") return "在线";
  if (s === "off") return "离线";
  return "未探测";
}
</script>

<template>
  <div class="hosts">
    <header class="bar glass-bar">
      <button v-if="view === 'login' || (view === 'setup' && hosts.length)" class="icon" @click="view = 'list'">‹</button>
      <button v-else-if="view === 'list' && authed" class="icon" @click="go('chat')">‹</button>
      <span v-else style="width: 34px"></span>
      <div class="title">
        {{ view === "boot" ? "北极星" : view === "list" ? "选择主机" : view === "setup" ? (step === 1 ? "登录账号" : "填授权码") : "登录主机" }}
      </div>
      <span style="width: 34px"></span>
    </header>

    <!-- 自动连接(回头客零打扰) -->
    <div v-if="view === 'boot'" class="body boot">
      <div class="logo pulse">✦</div>
      <p class="muted">{{ bootHint || "正在自动连接…" }}</p>
      <button class="btn ghost mt2" @click="view = 'list'; probeAll()">改用其它主机</button>
    </div>

    <!-- 主机列表 -->
    <div v-else-if="view === 'list'" class="body">
      <div
        v-for="h in hosts"
        :key="h.id"
        class="card host glass"
        :class="{ busy: busyHostId === h.id }"
        @click="tapHost(h)"
      >
        <div class="hicon">🖥️</div>
        <div class="hmeta">
          <div class="hname">{{ h.name }}</div>
          <div class="haddr faint">{{ hostLabel(h.base) }}</div>
          <div class="huser faint" v-if="h.user">
            {{ displayName(h.user) }} · {{ h.user.role === "owner" ? "管理员" : "成员" }}
            <span v-if="h.savedPassword" class="rememberd">· 已记住密码</span>
          </div>
        </div>
        <div class="hstate">
          <span class="dot" :class="status[h.id]"></span>
          <span class="slabel faint">{{ busyHostId === h.id ? "连接中…" : stateLabel(h) }}</span>
        </div>
        <button class="hdel" title="移除" @click="delHost(h, $event)">✕</button>
      </div>

      <button class="btn full add" :disabled="busy" @click="view = 'setup'; step = 1">＋ 添加主机</button>
      <button v-if="hosts.length" class="btn ghost full mt" :disabled="busy" @click="probeAll">
        刷新状态
      </button>

      <p class="foot faint">空壳客户端 · 文件与工作都在主机上 · 手机只远程访问与控制</p>
    </div>

    <!-- 首装向导:①账号 ②授权码 -->
    <div v-else-if="view === 'setup'" class="body">
      <div class="brand" v-if="!hosts.length && step === 1">
        <div class="logo">✦</div>
        <h1>北极星</h1>
        <p class="muted">先登录账号,再粘一次授权码 —— 以后都记住,直接进</p>
      </div>

      <div class="steps-dots">
        <span :class="{ on: step === 1 }">1 账号</span>
        <span class="sep">—</span>
        <span :class="{ on: step === 2 }">2 授权码</span>
      </div>

      <!-- 步① 账号密码 -->
      <div v-if="step === 1" class="card glass">
        <div class="tabs">
          <button :class="{ on: mode === 'login' }" @click="mode = 'login'">已有账号</button>
          <button :class="{ on: mode === 'signup' }" @click="mode = 'signup'">注册新账号</button>
        </div>
        <input
          v-model="username"
          class="field mt"
          placeholder="用户名"
          autocapitalize="off"
          autocorrect="off"
          spellcheck="false"
        />
        <input
          v-model="password"
          type="password"
          class="field mt"
          placeholder="密码"
          @keyup.enter="nextStep"
        />
        <input v-if="mode === 'signup'" v-model="nick" class="field mt" placeholder="昵称(可选)" />
        <label class="remember mt">
          <input type="checkbox" v-model="remember" />
          记住账号密码(下次自动登录)
        </label>
        <button class="btn full mt2" @click="nextStep">下一步</button>
      </div>

      <!-- 步② 授权码(只填这一次) -->
      <div v-else class="card glass">
        <label class="lbl">授权码 / 主机地址</label>
        <input
          v-model="addr"
          class="field mt"
          placeholder="PLRK1-…(自己的设备)/ PLRS1-…(受邀)/ IP:端口"
          autocapitalize="off"
          autocorrect="off"
          spellcheck="false"
          @keyup.enter="doSetup"
        />
        <label class="lbl mt2">备注名(可选)</label>
        <input v-model="addName" class="field mt" placeholder="比如:家里的台式机" />
        <button class="btn full mt2" :disabled="busy" @click="doSetup">
          {{ busy ? "连接中…" : "连接并登录" }}
        </button>
        <button class="btn ghost full mt" :disabled="busy" @click="step = 1">‹ 上一步</button>
        <p class="faint mt">授权码在主机端「互联」页复制,<b>只需要填这一次</b>,以后打开 App 自动连。</p>
      </div>
    </div>

    <!-- 老主机重登(预填记住的账号密码) -->
    <div v-else class="body">
      <div class="card glass">
        <div class="target faint">
          🖥️ {{ pendingHost?.name }} · {{ hostLabel(pendingHost?.base ?? "") }}
        </div>
        <input
          v-model="username"
          class="field mt"
          placeholder="用户名"
          autocapitalize="off"
          autocorrect="off"
          spellcheck="false"
        />
        <input
          v-model="password"
          type="password"
          class="field mt"
          placeholder="密码"
          @keyup.enter="doLogin"
        />
        <label class="remember mt">
          <input type="checkbox" v-model="remember" />
          记住账号密码(下次自动登录)
        </label>
        <button class="btn full mt2" :disabled="busy" @click="doLogin">
          {{ busy ? "请稍候…" : "登录" }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.hosts {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-height: 0;
}
.bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: calc(10px + var(--safe-top)) 12px 10px;
  border-bottom: 1px solid var(--line);
}
.title {
  font-weight: 600;
}
.icon {
  font-size: 26px;
  color: var(--text-dim);
  width: 34px;
}
.body {
  flex: 1;
  overflow-y: auto;
  padding: 18px 16px calc(24px + var(--safe-bottom));
}
.boot {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 14px;
}
.pulse {
  animation: pulse 1.4s ease-in-out infinite;
}
@keyframes pulse {
  50% {
    opacity: 0.35;
    transform: scale(0.92);
  }
}
.brand {
  text-align: center;
  margin: 26px 0;
}
.logo {
  font-size: 44px;
  color: var(--accent);
  line-height: 1;
}
.brand h1 {
  margin: 12px 0 6px;
  font-size: 26px;
  letter-spacing: 2px;
}
.steps-dots {
  display: flex;
  justify-content: center;
  gap: 10px;
  margin-bottom: 14px;
  font-size: 13px;
  color: var(--text-faint);
}
.steps-dots .on {
  color: var(--accent);
  font-weight: 700;
}
.steps-dots .sep {
  opacity: 0.5;
}
.card {
  background: var(--bg-elev);
  border: 1px solid var(--line);
  border-radius: var(--radius);
  padding: 16px;
}
.card.host {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 12px;
  position: relative;
  cursor: pointer;
}
.card.host:active {
  transform: scale(0.985);
}
.card.host.busy {
  opacity: 0.7;
}
.hicon {
  font-size: 30px;
}
.hmeta {
  flex: 1;
  min-width: 0;
}
.hname {
  font-weight: 600;
  font-size: 16px;
}
.haddr,
.huser {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.rememberd {
  color: var(--ok);
}
.hstate {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 3px;
  flex-shrink: 0;
  margin-right: 18px;
}
.dot {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  background: var(--text-faint);
}
.dot.on {
  background: var(--ok);
  box-shadow: 0 0 8px var(--ok);
}
.dot.off {
  background: var(--danger);
}
.dot.probing {
  background: #f5c451;
  animation: blink 1s infinite;
}
@keyframes blink {
  50% {
    opacity: 0.35;
  }
}
.slabel {
  font-size: 11px;
}
.hdel {
  position: absolute;
  top: 6px;
  right: 8px;
  color: var(--text-faint);
  font-size: 13px;
  padding: 4px;
}
.add {
  margin-top: 6px;
}
.lbl {
  font-size: 13px;
  color: var(--text-dim);
  display: block;
}
.tabs {
  display: flex;
  gap: 4px;
  background: var(--bg-elev2);
  border-radius: 12px;
  padding: 4px;
}
.tabs button {
  flex: 1;
  padding: 9px;
  border-radius: 9px;
  color: var(--text-dim);
  font-size: 14px;
}
.tabs button.on {
  background: var(--bg-elev);
  color: var(--text);
  font-weight: 600;
}
.remember {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13.5px;
  color: var(--text-dim);
  user-select: none;
}
.remember input {
  width: 17px;
  height: 17px;
  accent-color: var(--accent);
}
.target {
  padding-bottom: 4px;
}
.mt {
  margin-top: 10px;
}
.mt2 {
  margin-top: 16px;
}
.full {
  width: 100%;
}
.foot {
  text-align: center;
  margin-top: 26px;
}
</style>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { base, hostName, user, isOwner, displayName, logout } from "../lib/auth";
import { toggleableCaps, hasCap, setCap, type CapKey } from "../lib/capabilities";
import { go } from "../lib/nav";
import { invoke } from "../lib/net";
import { deviceNodeId } from "../lib/tunnel";
import {
  APP_VERSION,
  fetchLatest,
  hasUpdate,
  downloadUrls,
  type MobileLatest,
} from "../lib/update";

// hasCap/toggleableCaps 内部读响应式 user+capOverride,computed 自动联动
const caps = computed(() => toggleableCaps());

function toggle(key: CapKey) {
  setCap(key, !hasCap(key));
}
function isOn(key: CapKey) {
  return hasCap(key);
}

// ── 账号根口令(个人设备联盟身份锚) ──
// owner 登录后从主机取本机账号根;在新设备上输入它即可加入同一联盟。属根凭据,默认掩码。
interface AccountRoot {
  code: string;
  anchor: string;
}
const accountRoot = ref<AccountRoot | null>(null);
const revealed = ref(false);
const copied = ref(false);

// 本设备 iroh 标识(NodeId):首次经 iroh 连主机前,把它给主机「授权新设备」加白名单。
const devNode = ref<string | null>(null);
const devCopied = ref(false);
async function copyDevNode() {
  if (!devNode.value) return;
  try {
    await navigator.clipboard.writeText(devNode.value);
    devCopied.value = true;
    setTimeout(() => (devCopied.value = false), 1500);
  } catch {
    /* 剪贴板不可用 */
  }
}

onMounted(async () => {
  devNode.value = await deviceNodeId(); // 原生隧道可用则返回本设备 NodeId,否则 null
  if (isOwner) {
    try {
      accountRoot.value = await invoke<AccountRoot>("collab_account_root");
    } catch {
      /* 主机未开放或非 owner:静默,不显示卡片 */
    }
  }
});

// ── 远程更新检查(CF/GitHub 三端点,见 lib/update.ts) ──
const checking = ref(false);
const checked = ref(false);
const latest = ref<MobileLatest | null>(null);
const updateAvail = computed(() => !!latest.value && hasUpdate(latest.value));
const dlUrls = computed(() => (latest.value ? downloadUrls(latest.value) : []));

async function checkUpdate() {
  if (checking.value) return;
  checking.value = true;
  checked.value = false;
  try {
    latest.value = await fetchLatest();
  } finally {
    checking.value = false;
    checked.value = true;
  }
}

const maskedCode = computed(() => {
  const c = accountRoot.value?.code ?? "";
  // 保留 PLRA1- 前缀,其余字符掩成圆点,格式不变(便于识别但不泄露)。
  const i = c.indexOf("-");
  const head = i >= 0 ? c.slice(0, i + 1) : "";
  return head + c.slice(head.length).replace(/[A-Za-z0-9]/g, "•");
});

async function copyRoot() {
  const c = accountRoot.value?.code;
  if (!c) return;
  try {
    await navigator.clipboard.writeText(c);
    copied.value = true;
    setTimeout(() => (copied.value = false), 1500);
  } catch {
    /* 剪贴板不可用时忽略 */
  }
}
</script>

<template>
  <div class="settings">
    <header class="bar">
      <button class="back" title="返回对话" @click="go('chat')">‹</button>
      <div class="title">我的</div>
      <span style="width: 34px"></span>
    </header>
    <div class="scroll">
      <div class="profile">
        <div class="avatar">{{ displayName(user).slice(0, 1) }}</div>
        <div class="pmeta">
          <div class="pname">{{ displayName(user) }}</div>
          <div class="faint">
            {{ isOwner ? "管理员 · owner" : "成员 · member" }}
          </div>
        </div>
      </div>

      <div v-if="accountRoot" class="rootcard">
        <div class="rc-head">
          <span class="rc-ic">🔑</span>
          <span class="rc-title">账号根口令</span>
          <span class="rc-badge">设备联盟</span>
        </div>
        <div class="rc-code" @click="revealed = !revealed">
          {{ revealed ? accountRoot.code : maskedCode }}
        </div>
        <div class="rc-actions">
          <button class="rc-btn" @click="revealed = !revealed">{{ revealed ? "隐藏" : "显示" }}</button>
          <button class="rc-btn" @click="copyRoot">{{ copied ? "已复制 ✓" : "复制" }}</button>
        </div>
        <p class="rc-hint">在新设备上输入此口令即可加入你的设备联盟,互相调用存储与算力。这是根凭据,请妥善保管、勿外泄。</p>
      </div>

      <div class="group">
        <div class="glabel">连接</div>
        <div class="krow">
          <span class="muted">当前主机</span>
          <span class="kval">{{ hostName ? `${hostName} · ` : "" }}{{ base || "—" }}</span>
        </div>
        <button class="btn ghost full mt" @click="go('hosts')">⇄ 切换 / 管理主机</button>
      </div>

      <div v-if="devNode" class="group">
        <div class="glabel">本设备标识(iroh · 授权用)</div>
        <div class="devnode" @click="copyDevNode">{{ devNode }}</div>
        <button class="btn ghost full mt" @click="copyDevNode">{{ devCopied ? "已复制 ✓" : "复制本设备标识" }}</button>
        <p class="faint gp" style="margin-top: 8px">首次经 iroh 连主机前,在电脑端「互联 · 授权新设备」里粘这串;主机放行后即可 P2P 打洞直连。</p>
      </div>

      <div class="group">
        <div class="glabel">能力管控</div>
        <p class="faint gp">关闭的能力会从侧边栏与输入框「＋」面板隐藏。核心对话不可关闭。</p>
        <div v-for="c in caps" :key="c.key" class="caprow" @click="toggle(c.key)">
          <span class="cic">{{ c.icon }}</span>
          <span class="cmeta">
            <span class="clabel">{{ c.label }}</span>
            <span class="faint">{{ c.desc }}</span>
          </span>
          <span class="switch" :class="{ on: isOn(c.key) }"><i></i></span>
        </div>
      </div>

      <div class="group">
        <div class="glabel">关于与更新</div>
        <div class="krow">
          <span class="muted">当前版本</span>
          <span class="kval">v{{ APP_VERSION }}</span>
        </div>
        <button class="btn ghost full mt" :disabled="checking" @click="checkUpdate">
          {{ checking ? "检查中…" : "🔄 检查更新" }}
        </button>
        <div v-if="updateAvail && latest" class="upcard">
          <div class="upver">🎉 发现新版 v{{ latest.version }}</div>
          <p v-if="latest.notes" class="faint upnotes">{{ latest.notes }}</p>
          <!-- 顶层导航到外部域 → Capacitor 交给系统浏览器下载,浏览器里点开即装 -->
          <a class="btn full dl" :href="dlUrls[0]" target="_blank">⬇ 下载新版 APK</a>
          <a v-if="dlUrls[1]" class="mirror faint" :href="dlUrls[1]" target="_blank">
            下载慢?试试备用地址
          </a>
          <p class="faint uphint">
            将用系统浏览器下载,完成后点开安装(首次需允许「安装未知来源应用」)。
          </p>
        </div>
        <p v-else-if="checked && latest" class="faint upres">已是最新版本 ✓</p>
        <p v-else-if="checked && !latest" class="faint upres">
          暂时获取不到更新信息,请检查外网连接后再试。
        </p>
      </div>

      <button class="btn ghost logout" @click="logout">退出登录</button>
      <p class="faint ver">北极星 · 安卓远程壳 v{{ APP_VERSION }}</p>
    </div>
  </div>
</template>

<style scoped>
.settings {
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
.back {
  font-size: 26px;
  color: var(--text-dim);
  width: 34px;
}
.title {
  font-weight: 600;
}
.scroll {
  flex: 1;
  overflow-y: auto;
  padding: 16px 14px 30px;
}
.profile {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 6px 4px 18px;
}
.avatar {
  width: 52px;
  height: 52px;
  border-radius: 50%;
  background: var(--accent);
  color: #fff;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 22px;
  font-weight: 600;
}
.pname {
  font-size: 18px;
  font-weight: 600;
}
.group {
  background: var(--bg-elev);
  border: 1px solid var(--line);
  border-radius: 14px;
  padding: 14px;
  margin-bottom: 14px;
}
/* 账号根口令:显眼卡片(渐变描边 + 等宽口令) */
.rootcard {
  position: relative;
  border-radius: 16px;
  padding: 16px;
  margin-bottom: 16px;
  background: var(--bg-elev);
  border: 1.5px solid transparent;
  background-image:
    linear-gradient(var(--bg-elev), var(--bg-elev)),
    linear-gradient(135deg, #0d99ff, #7c4dff);
  background-origin: border-box;
  background-clip: padding-box, border-box;
  box-shadow: 0 6px 22px rgba(13, 153, 255, 0.14);
}
.rc-head {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 12px;
}
.rc-ic {
  font-size: 18px;
}
.rc-title {
  font-weight: 600;
  font-size: 15px;
}
.rc-badge {
  margin-left: auto;
  font-size: 11px;
  font-weight: 700;
  color: #fff;
  background: linear-gradient(135deg, #0d99ff, #7c4dff);
  padding: 3px 10px;
  border-radius: 999px;
}
.rc-code {
  font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
  font-size: 17px;
  letter-spacing: 1.5px;
  text-align: center;
  padding: 13px 8px;
  border-radius: 11px;
  background: var(--bg-elev2);
  border: 1px solid var(--line);
  user-select: all;
  word-break: break-all;
  cursor: pointer;
}
.rc-actions {
  display: flex;
  gap: 10px;
  margin-top: 11px;
}
.rc-btn {
  flex: 1;
  padding: 9px;
  border-radius: 10px;
  border: 1px solid var(--line);
  background: var(--bg-elev2);
  color: var(--text);
  font-size: 13.5px;
}
.rc-btn:active {
  opacity: 0.7;
}
.rc-hint {
  margin: 12px 0 0;
  font-size: 12px;
  line-height: 1.6;
  color: var(--text-dim);
}
.devnode {
  font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
  font-size: 12px;
  word-break: break-all;
  padding: 11px 12px;
  border-radius: 10px;
  background: var(--bg-elev2);
  border: 1px solid var(--line);
  user-select: all;
  cursor: pointer;
  line-height: 1.5;
}
.glabel {
  font-size: 13px;
  color: var(--text-dim);
  margin-bottom: 8px;
}
.gp {
  margin: 0 0 10px;
}
.krow {
  display: flex;
  justify-content: space-between;
  gap: 12px;
}
.kval {
  color: var(--text-dim);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 60%;
}
.caprow {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 11px 0;
  border-top: 1px solid var(--line);
}
.cic {
  font-size: 22px;
}
.cmeta {
  flex: 1;
  display: flex;
  flex-direction: column;
}
.clabel {
  font-weight: 500;
}
.switch {
  width: 44px;
  height: 26px;
  border-radius: 13px;
  background: var(--bg-elev2);
  border: 1px solid var(--line);
  position: relative;
  transition: background 0.15s;
  flex-shrink: 0;
}
.switch i {
  position: absolute;
  top: 2px;
  left: 2px;
  width: 20px;
  height: 20px;
  border-radius: 50%;
  background: var(--text-faint);
  transition: transform 0.15s, background 0.15s;
}
.switch.on {
  background: var(--accent);
  border-color: var(--accent);
}
.switch.on i {
  transform: translateX(18px);
  background: #fff;
}
/* 更新卡片 */
.upcard {
  margin-top: 12px;
  padding: 13px;
  border-radius: 12px;
  background: var(--accent-soft);
  border: 1px solid var(--accent);
}
.upver {
  font-weight: 600;
  font-size: 15px;
}
.upnotes {
  margin: 6px 0 0;
  white-space: pre-line;
}
.dl {
  margin-top: 10px;
  text-decoration: none;
}
.mirror {
  display: block;
  text-align: center;
  margin-top: 8px;
  text-decoration: none;
}
.uphint {
  margin: 8px 0 0;
  font-size: 11.5px;
  line-height: 1.5;
}
.upres {
  margin: 8px 0 0;
  text-align: center;
}
.logout {
  width: 100%;
  margin-top: 6px;
  color: var(--danger);
}
.full {
  width: 100%;
}
.mt {
  margin-top: 10px;
}
.ver {
  text-align: center;
  margin-top: 18px;
}
</style>

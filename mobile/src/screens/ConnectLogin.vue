<script setup lang="ts">
import { ref } from "vue";
import { base, connectHost, login, signup, bootstrap, redeem } from "../lib/auth";
import { parseShareCode, probeHost } from "../lib/net";
import { toast, toastErr } from "../lib/toast";
import { initChat } from "../lib/chat";

type Mode = "login" | "signup" | "redeem";
const mode = ref<Mode>("login");
const host = ref(base.value);
const connected = ref(!!base.value);
const busy = ref(false);

const username = ref("");
const password = ref("");
const displayName = ref("");
const code = ref("");

async function doConnect() {
  const raw = host.value.trim();
  if (!raw) return toast("请填写主机地址或粘贴分享码", "error");
  busy.value = true;
  try {
    // 分享码:自动探活其中可达地址
    const share = parseShareCode(raw);
    if (share) {
      const ok = await probeHost(share.addrs);
      if (!ok) throw new Error("分享码里的地址都连不上");
      host.value = ok;
      await connectHost(ok);
      if (share.code) {
        code.value = share.code;
        mode.value = "redeem";
      }
    } else {
      const addr = /^https?:\/\//.test(raw) ? raw : `http://${raw}`;
      host.value = addr;
      await connectHost(addr);
    }
    connected.value = true;
    toast("主机已连接", "ok");
  } catch (e) {
    toastErr(e);
  } finally {
    busy.value = false;
  }
}

async function submit() {
  if (!connected.value) return doConnect();
  busy.value = true;
  try {
    if (mode.value === "login") {
      await login(username.value.trim(), password.value);
    } else if (mode.value === "signup") {
      await signup(username.value.trim(), password.value, displayName.value.trim() || username.value.trim());
    } else {
      await redeem({
        code: code.value.trim(),
        username: username.value.trim(),
        password: password.value,
        displayName: displayName.value.trim() || username.value.trim(),
      });
    }
    initChat();
  } catch (e) {
    // 首个账号:后端未初始化 → 引导 bootstrap 建 owner
    const msg = (e as Error).message || "";
    if (mode.value === "signup" && /初始化|bootstrap|owner|无账号|未初始化/.test(msg)) {
      try {
        await bootstrap(username.value.trim(), password.value, displayName.value.trim() || username.value.trim());
        initChat();
        return;
      } catch (e2) {
        toastErr(e2);
      }
    } else {
      toastErr(e);
    }
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="login">
    <div class="brand">
      <div class="logo">✦</div>
      <h1>北极星</h1>
      <p class="muted">登录账号，远程连接你的中控平台</p>
    </div>

    <div class="card">
      <label class="lbl">主机地址 / 分享码</label>
      <div class="host-row">
        <input
          v-model="host"
          class="field"
          placeholder="192.168.1.10:8787 或 PLRS1-…"
          autocapitalize="off"
          autocorrect="off"
          spellcheck="false"
          @change="connected = false"
        />
        <button class="btn ghost connect" :disabled="busy" @click="doConnect">
          {{ connected ? "✓" : "连接" }}
        </button>
      </div>

      <template v-if="connected">
        <div class="tabs">
          <button :class="{ on: mode === 'login' }" @click="mode = 'login'">登录</button>
          <button :class="{ on: mode === 'signup' }" @click="mode = 'signup'">注册</button>
          <button :class="{ on: mode === 'redeem' }" @click="mode = 'redeem'">邀请码</button>
        </div>

        <input
          v-if="mode === 'redeem'"
          v-model="code"
          class="field mt"
          placeholder="邀请码"
          autocapitalize="off"
        />
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
          @keyup.enter="submit"
        />
        <input
          v-if="mode !== 'login'"
          v-model="displayName"
          class="field mt"
          placeholder="昵称（可选）"
        />

        <button class="btn full mt" :disabled="busy" @click="submit">
          {{ busy ? "请稍候…" : mode === "login" ? "登录" : mode === "signup" ? "注册并登录" : "凭码入伙" }}
        </button>
      </template>
      <p v-else class="faint mt">
        先连接主机。局域网填 IP:端口；外网填域名；有分享码直接粘贴即可自动定位。
      </p>
    </div>

    <p class="foot faint">空壳客户端 · 不在本机运行后端 · 所有工作在中控平台完成</p>
  </div>
</template>

<style scoped>
.login {
  flex: 1;
  display: flex;
  flex-direction: column;
  justify-content: center;
  padding: 24px 22px calc(24px + var(--safe-bottom));
  padding-top: calc(24px + var(--safe-top));
}
.brand {
  text-align: center;
  margin-bottom: 26px;
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
.card {
  background: var(--bg-elev);
  border: 1px solid var(--line);
  border-radius: var(--radius);
  padding: 18px;
}
.lbl {
  font-size: 13px;
  color: var(--text-dim);
}
.host-row {
  display: flex;
  gap: 8px;
  margin-top: 8px;
}
.connect {
  padding: 0 16px;
  white-space: nowrap;
}
.tabs {
  display: flex;
  gap: 4px;
  margin-top: 16px;
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
.mt {
  margin-top: 10px;
}
.full {
  width: 100%;
}
.foot {
  text-align: center;
  margin-top: 22px;
}
</style>

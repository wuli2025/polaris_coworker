<script setup lang="ts">
/**
 * 全应用统一的账号管理入口。
 *
 * 登录仍只由 EmailLogin 负责；这里汇总「我是谁 / 设备 / 验证码邮件 / 退出登录」，
 * 避免协作、互联各自再造一套账号按钮。SMTP 只是 owner 的验证码投递配置，
 * 不再作为一个与账号并列的「邮箱服务」出现。
 */
import { computed, onBeforeUnmount, onMounted, reactive, ref } from "vue";
import {
  AtSign,
  Check,
  ChevronDown,
  CircleUserRound,
  KeyRound,
  LoaderCircle,
  LogOut,
  MailCheck,
  MonitorSmartphone,
  ShieldCheck,
  X,
} from "@lucide/vue";
import { useAccountStore } from "../../stores/account";
import { collabApi } from "../collab/api";
import { errMsg } from "../../lib/err";
import { toast } from "../../composables/useToast";

const emit = defineEmits<{
  (e: "close"): void;
  (e: "logged-out"): void;
}>();

const account = useAccountStore();
const mailOpen = ref(false);
const mailLoaded = ref(false);
const mailBusy = ref(false);
const mailErr = ref("");
const logoutBusy = ref(false);

const mailCfg = reactive({
  host: "smtp.qq.com",
  port: 465,
  user: "",
  pass: "",
  from: "",
  signupOpen: true,
  passSet: false,
  configured: false,
  testTo: "",
});

const deviceSummary = computed(() => {
  if (!account.devices.length) return "当前设备";
  return `${account.devices.length} 台设备 · ${account.onlineCount} 台在线`;
});

async function loadMailConfig() {
  if (mailLoaded.value || mailBusy.value) return;
  mailBusy.value = true;
  mailErr.value = "";
  try {
    const cfg = await collabApi.adminEmailConfig();
    mailCfg.host = cfg.host || "smtp.qq.com";
    mailCfg.port = cfg.port || 465;
    mailCfg.user = cfg.user || "";
    mailCfg.from = cfg.from || "";
    mailCfg.signupOpen = cfg.signupOpen;
    mailCfg.passSet = cfg.passSet;
    mailCfg.configured = cfg.configured;
    mailCfg.pass = "";
    mailLoaded.value = true;
  } catch (e) {
    mailErr.value = errMsg(e);
  } finally {
    mailBusy.value = false;
  }
}

function toggleMail() {
  mailOpen.value = !mailOpen.value;
  if (mailOpen.value) void loadMailConfig();
}

async function saveMailConfig() {
  mailErr.value = "";
  if (!mailCfg.user.trim()) {
    mailErr.value = "请填写发信邮箱";
    return;
  }
  if (!mailCfg.passSet && !mailCfg.pass.trim()) {
    mailErr.value = "请填写 SMTP 授权码（不是邮箱登录密码）";
    return;
  }
  mailBusy.value = true;
  try {
    const result = await collabApi.adminEmailConfigSet({
      host: mailCfg.host.trim() || "smtp.qq.com",
      port: Number(mailCfg.port) || 465,
      user: mailCfg.user.trim(),
      pass: mailCfg.pass,
      from: mailCfg.from.trim(),
      signupOpen: mailCfg.signupOpen,
      testTo: mailCfg.testTo.trim() || undefined,
    });
    mailCfg.passSet = true;
    mailCfg.configured = result.configured;
    mailCfg.pass = "";
    toast.info(
      mailCfg.testTo.trim()
        ? "验证码邮箱已保存，测试邮件已发出"
        : "验证码邮箱已保存"
    );
    mailCfg.testTo = "";
  } catch (e) {
    mailErr.value = errMsg(e);
  } finally {
    mailBusy.value = false;
  }
}

async function logout() {
  if (
    !confirm(
      "退出登录？\n\n本机会同时退出协作和设备网；自动挂载的远程盘会卸载。重新用同一邮箱登录即可恢复，权限设置仍会保留。"
    )
  )
    return;
  logoutBusy.value = true;
  try {
    await account.logout();
    toast.info("已退出登录");
    emit("logged-out");
    emit("close");
  } catch (e) {
    toast.error(errMsg(e));
  } finally {
    logoutBusy.value = false;
  }
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === "Escape" && !logoutBusy.value) emit("close");
}

onMounted(() => {
  void account.refresh();
  window.addEventListener("keydown", onKeydown);
});
onBeforeUnmount(() => window.removeEventListener("keydown", onKeydown));
</script>

<template>
  <Teleport to="body">
    <div class="account-mask" @click.self="emit('close')">
      <section class="account-panel" role="dialog" aria-modal="true" aria-label="账号管理">
        <header class="account-head">
          <span class="account-icon"><CircleUserRound :size="20" :stroke-width="1.8" /></span>
          <div class="account-heading">
            <h2>账号管理</h2>
            <p>一个邮箱，一份身份，协作与互联共用</p>
          </div>
          <button class="icon-btn" title="关闭" :disabled="logoutBusy" @click="emit('close')">
            <X :size="17" :stroke-width="2" />
          </button>
        </header>

        <div class="account-body">
          <section class="identity-card">
            <div class="avatar"><AtSign :size="21" :stroke-width="1.8" /></div>
            <div class="identity-main">
              <strong>{{ account.label || "已登录账号" }}</strong>
              <span v-if="account.uid" class="uid" :title="account.uid">ID {{ account.uid }}</span>
              <span v-else class="uid">本机会话</span>
            </div>
            <span class="role" :class="{ owner: account.isOwner }">
              <ShieldCheck :size="12" />{{ account.isOwner ? "管理者" : account.role || "成员" }}
            </span>
          </section>

          <div class="facts">
            <div class="fact">
              <MailCheck :size="15" />
              <span><small>登录邮箱</small><b>{{ account.email || "当前会话未返回邮箱" }}</b></span>
            </div>
            <div class="fact">
              <MonitorSmartphone :size="15" />
              <span><small>设备</small><b>{{ deviceSummary }}</b></span>
            </div>
          </div>

          <section v-if="account.isOwner" class="mail-section">
            <button class="section-toggle" :class="{ open: mailOpen }" @click="toggleMail">
              <span class="toggle-icon"><KeyRound :size="15" /></span>
              <span class="toggle-copy">
                <b>验证码邮件</b>
                <small>管理员设置 · 登录验证码与账号找回邮件</small>
              </span>
              <span v-if="mailLoaded" class="status" :class="{ ok: mailCfg.configured }">
                {{ mailCfg.configured ? "已配置" : "未配置" }}
              </span>
              <LoaderCircle v-if="mailBusy && !mailLoaded" :size="14" class="spin" />
              <ChevronDown v-else :size="15" class="chevron" />
            </button>

            <div v-if="mailOpen" class="mail-form">
              <p class="mail-note">
                这里配置的是系统发送验证码所用的邮箱，不会改变你的登录账号。
              </p>
              <div class="host-row">
                <label>
                  <span>SMTP 服务器</span>
                  <input v-model.trim="mailCfg.host" placeholder="smtp.qq.com" />
                </label>
                <label class="port-field">
                  <span>端口</span>
                  <input v-model.number="mailCfg.port" inputmode="numeric" placeholder="465" />
                </label>
              </div>
              <label>
                <span>发信邮箱</span>
                <input v-model.trim="mailCfg.user" type="email" autocomplete="off" placeholder="name@example.com" />
              </label>
              <label>
                <span>SMTP 授权码</span>
                <input
                  v-model="mailCfg.pass"
                  type="password"
                  autocomplete="new-password"
                  :placeholder="mailCfg.passSet ? '已配置，留空不改' : '邮箱后台生成的 SMTP 授权码'"
                />
              </label>
              <label>
                <span>发件人地址（可选）</span>
                <input v-model.trim="mailCfg.from" type="email" autocomplete="off" placeholder="默认与发信邮箱相同" />
              </label>
              <label class="check-row">
                <input v-model="mailCfg.signupOpen" type="checkbox" />
                <span>允许邮箱验证码自助登录 / 首次自动开户</span>
              </label>
              <label>
                <span>测试收件邮箱（可选）</span>
                <input v-model.trim="mailCfg.testTo" type="email" autocomplete="off" placeholder="保存时发送一封测试邮件" />
              </label>
              <p v-if="mailErr" class="form-error">{{ mailErr }}</p>
              <div class="mail-actions">
                <span class="auth-hint">QQ 邮箱请填写“授权码”，不要填写 QQ 密码。</span>
                <button class="save-btn" :disabled="mailBusy" @click="saveMailConfig">
                  <LoaderCircle v-if="mailBusy" :size="14" class="spin" />
                  <Check v-else :size="14" />
                  {{ mailCfg.testTo.trim() ? "保存并测试" : "保存" }}
                </button>
              </div>
            </div>
          </section>
        </div>

        <footer class="account-foot">
          <span>退出会同时断开协作会话和本机设备网。</span>
          <button class="logout-btn" :disabled="logoutBusy" @click="logout">
            <LoaderCircle v-if="logoutBusy" :size="14" class="spin" />
            <LogOut v-else :size="14" />
            退出登录
          </button>
        </footer>
      </section>
    </div>
  </Teleport>
</template>

<style scoped>
.account-mask {
  position: fixed;
  inset: 0;
  z-index: 120;
  display: grid;
  place-items: center;
  padding: 22px;
  background: var(--overlay);
  -webkit-backdrop-filter: blur(7px);
  backdrop-filter: blur(7px);
}
.account-panel {
  width: min(590px, 96vw);
  max-height: min(820px, 92vh);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  color: var(--text);
  background: color-mix(in srgb, var(--panel) 96%, transparent);
  border: 1px solid var(--border);
  border-radius: 18px;
  box-shadow: var(--shadow-lg);
}
.account-head {
  display: flex;
  align-items: center;
  gap: 11px;
  padding: 17px 18px 14px;
  border-bottom: 1px solid var(--border-soft);
}
.account-icon,
.toggle-icon {
  display: grid;
  place-items: center;
  flex: none;
  color: var(--primary);
  background: var(--primary-soft);
  border: 1px solid color-mix(in srgb, var(--primary) 22%, transparent);
}
.account-icon { width: 36px; height: 36px; border-radius: 11px; }
.account-heading { flex: 1; min-width: 0; }
.account-heading h2 { margin: 0; font-size: 16px; font-weight: 680; }
.account-heading p { margin: 2px 0 0; color: var(--muted); font-size: 12px; line-height: 1.45; }
.icon-btn {
  width: 32px;
  height: 32px;
  display: grid;
  place-items: center;
  border: 1px solid transparent;
  border-radius: 9px;
  color: var(--text-2);
  background: transparent;
}
.icon-btn:hover:not(:disabled) { background: var(--selection-bg); color: var(--text); }
.account-body { padding: 16px 18px 18px; overflow-y: auto; }
.identity-card {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 13px;
  border: 1px solid var(--border-soft);
  border-radius: 13px;
  background: var(--bg-soft);
}
.avatar {
  width: 40px;
  height: 40px;
  display: grid;
  place-items: center;
  flex: none;
  border-radius: 12px;
  color: var(--primary);
  background: var(--panel);
  border: 1px solid var(--border);
}
.identity-main { display: flex; flex: 1; min-width: 0; flex-direction: column; gap: 1px; }
.identity-main strong { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 14px; }
.uid { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--muted); font-size: 11.5px; }
.role {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 3px 8px;
  border-radius: 999px;
  color: var(--text-2);
  background: var(--selection-bg);
  font-size: 11px;
  font-weight: 650;
}
.role.owner { color: var(--primary); background: var(--primary-soft); }
.facts { display: grid; grid-template-columns: 1fr 1fr; gap: 9px; margin-top: 10px; }
.fact {
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 10px 11px;
  color: var(--text-2);
  border: 1px solid var(--border-soft);
  border-radius: 11px;
  background: var(--panel);
}
.fact > svg { flex: none; color: var(--muted); }
.fact span { min-width: 0; display: flex; flex-direction: column; }
.fact small { color: var(--muted); font-size: 10.5px; }
.fact b { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12px; font-weight: 600; }
.mail-section { margin-top: 13px; border: 1px solid var(--border-soft); border-radius: 13px; overflow: hidden; }
.section-toggle {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 11px 12px;
  border: 0;
  color: var(--text);
  background: var(--panel);
  text-align: left;
}
.section-toggle:hover { background: var(--panel-hover); }
.toggle-icon { width: 30px; height: 30px; border-radius: 9px; }
.toggle-copy { flex: 1; min-width: 0; display: flex; flex-direction: column; }
.toggle-copy b { font-size: 12.5px; }
.toggle-copy small { color: var(--muted); font-size: 10.8px; }
.status { padding: 2px 7px; border-radius: 999px; color: var(--muted); background: var(--selection-bg); font-size: 10.5px; }
.status.ok { color: var(--ok); background: var(--ok-soft); }
.chevron { color: var(--muted); transition: transform .16s; }
.section-toggle.open .chevron { transform: rotate(180deg); }
.mail-form { display: flex; flex-direction: column; gap: 9px; padding: 12px; border-top: 1px solid var(--border-soft); background: var(--bg-soft); }
.mail-note { margin: 0 0 2px; color: var(--text-2); font-size: 11.5px; line-height: 1.55; }
.mail-form label { display: flex; flex-direction: column; gap: 4px; color: var(--text-2); font-size: 11px; }
.mail-form input:not([type="checkbox"]) {
  width: 100%;
  height: 35px;
  padding: 0 10px;
  border: 1px solid var(--border);
  border-radius: 9px;
  outline: none;
  color: var(--text);
  background: var(--panel);
  font-size: 12.5px;
}
.mail-form input::placeholder { color: var(--dim); opacity: 1; }
.mail-form input:focus { border-color: var(--primary); box-shadow: 0 0 0 3px var(--primary-soft); }
.host-row { display: grid; grid-template-columns: 1fr 92px; gap: 9px; }
.check-row { flex-direction: row !important; align-items: center; gap: 7px !important; padding: 2px 0; }
.check-row input { width: 15px; height: 15px; accent-color: var(--primary); }
.form-error { margin: 0; padding: 7px 9px; color: var(--vermilion); background: var(--vermilion-soft); border-radius: 8px; font-size: 11.5px; }
.mail-actions { display: flex; align-items: center; justify-content: space-between; gap: 10px; }
.auth-hint { flex: 1; color: var(--muted); font-size: 10.5px; line-height: 1.45; }
.save-btn,
.logout-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  height: 34px;
  padding: 0 13px;
  border-radius: 9px;
  font-size: 12px;
  font-weight: 650;
}
.save-btn { border: 0; color: var(--btn-solid-text); background: var(--btn-solid-bg); }
.account-foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 12px 18px;
  border-top: 1px solid var(--border-soft);
  background: var(--bg-soft);
}
.account-foot > span { color: var(--muted); font-size: 11px; }
.logout-btn { flex: none; color: var(--vermilion); background: var(--vermilion-soft); border: 1px solid color-mix(in srgb, var(--vermilion) 24%, transparent); }
.save-btn:disabled,
.logout-btn:disabled,
.icon-btn:disabled { opacity: .55; cursor: default; }
.spin { animation: spin .9s linear infinite; }
@keyframes spin { to { transform: rotate(360deg); } }
@media (max-width: 620px) {
  .account-mask { padding: 10px; align-items: end; }
  .account-panel { width: 100%; max-height: 94vh; border-radius: 17px 17px 10px 10px; }
  .facts { grid-template-columns: 1fr; }
  .host-row { grid-template-columns: 1fr 82px; }
  .account-foot { align-items: stretch; flex-direction: column; }
  .logout-btn { width: 100%; }
}
</style>

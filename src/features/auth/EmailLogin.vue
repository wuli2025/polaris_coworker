<script setup lang="ts">
/**
 * EmailLogin.vue —— 全应用**唯一**的登录界面。
 *
 * 在此之前,用户要面对四个入口:云端注册页、本机登录页、「设为主机」、以及互联页里
 * 那张还要再填一遍账号密码的「入网」表单。于是就有了那句「怎么还要登两个账号,还分什么
 * 云端本地」。这里把它们压成两个输入框:**邮箱**和**六位验证码**。
 *
 * 两条运行时路径,对用户是同一件事:
 *  · 桌面(Tauri):一条 `account_login_code` 命令办完四步 —— 换断言 → 钉账号中心 →
 *    通过本机准入 → 入网并原子保存设备密钥/UID。回来时这台机器已经在设备网里了。
 *  · 网页/手机壳:走 HTTP —— 账号中心换断言 → 当前主机验签换会话。没有设备网那一层
 *    (浏览器里没有 iroh),但账号是同一个。
 *
 * 密码在这个组件里**一次都没出现**。它在服务端仍然存在(CI/脚本/探针要用),但用户
 * 再也不需要知道有这么个东西。
 */
import { computed, onUnmounted, ref, watch } from "vue";
import { LoaderCircle, Mail, ShieldCheck, ArrowLeft } from "@lucide/vue";
import { invoke, isTauri } from "../../tauri";
import { useCollabStore } from "../collab/stores/collab";
import { useAccountStore } from "../../stores/account";
import { errMsg } from "../../lib/err";

const props = withDefaults(
  defineProps<{
    /** 标题下那句话。不同页面进来语境不同,让调用方自己说。 */
    lead?: string;
    /** 桌面路径:登录成功后顺带把这台机器接进设备网(互联页要,协作页不要)。 */
    joinMesh?: boolean;
    /**
     * 预填邮箱。粘连接码时认出「这是 xxx@qq.com 的设备网」后带进来 ——
     * 用户已经证明他知道这个账号了,再让他手打一遍纯属添堵。
     */
    prefill?: string;
  }>(),
  {
    lead: "填邮箱收验证码就能用 —— 你的所有设备用同一个账号,登录后自己就连上了。",
    joinMesh: true,
    prefill: "",
  }
);

const emit = defineEmits<{ (e: "done", info: Record<string, unknown>): void }>();

const collab = useCollabStore();
const account = useAccountStore();

const step = ref<"email" | "code">("email");
const email = ref(props.prefill);
// 弹层是 v-if 挂载的,但同一个实例可能被复用(先自己开、再由粘码带邮箱开)——
// 所以 prefill 变了要跟着填,不能只在初始化时读一次。用户已经打过字的不覆盖。
watch(
  () => props.prefill,
  (v) => {
    if (v && !email.value) email.value = v;
  }
);
const code = ref("");
/** 协作者加入**别人**的主机时才要;自己的设备永远用不上,所以默认折叠。 */
const inviteCode = ref("");
const showInvite = ref(false);
/** 自建账号中心的人才需要;默认空 = 官方云端账号中心。 */
const authorityUrl = ref("");
const allowInsecureHttp = ref(false);
const showAdvanced = ref(false);

const busy = ref(false);
const err = ref("");
const cooldown = ref(0);
let timer: ReturnType<typeof setInterval> | null = null;
onUnmounted(() => {
  if (timer) clearInterval(timer);
});

const emailOk = computed(() => /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email.value.trim()));
const codeOk = computed(() => /^\d{6}$/.test(code.value.trim()));

function startCooldown() {
  cooldown.value = 60;
  if (timer) clearInterval(timer);
  timer = setInterval(() => {
    cooldown.value--;
    if (cooldown.value <= 0 && timer) {
      clearInterval(timer);
      timer = null;
    }
  }, 1000);
}

async function sendCode() {
  if (!emailOk.value) {
    err.value = "邮箱格式不对";
    return;
  }
  if (cooldown.value > 0 || busy.value) return;
  err.value = "";
  busy.value = true;
  try {
    if (isTauri && props.joinMesh) {
      await invoke("account_send_code", {
        email: email.value.trim(),
        url: authorityUrl.value.trim() || null,
        allowInsecureHttp: allowInsecureHttp.value,
      });
    } else {
      await collab.sendLoginCode(email.value.trim(), allowInsecureHttp.value);
    }
    step.value = "code";
    startCooldown();
  } catch (e) {
    const m = errMsg(e);
    if (m.includes("明文 HTTP") || m.includes("HTTPS")) showAdvanced.value = true;
    err.value = m;
  } finally {
    busy.value = false;
  }
}

async function submit() {
  if (!codeOk.value) {
    err.value = "请填 6 位验证码";
    return;
  }
  err.value = "";
  busy.value = true;
  try {
    if (isTauri && props.joinMesh) {
      // 桌面:一条命令办完四步。先把本机主机起起来(幂等)——
      // 别的设备要连进来,得有人在门口听着;这一步失败不阻断登录,只是"暂时连不进来"。
      try {
        await invoke("collab_host_start");
      } catch {
        /* 已在跑 / 端口被占:登录照走,互联页会显示实情 */
      }
      const r = (await invoke("account_login_code", {
        email: email.value.trim(),
        code: code.value.trim(),
        url: authorityUrl.value.trim() || null,
        // 只有「这台机器已经有别人当主人」时才用得上。以前这个框在桌面这条路上
        // 压根没被读过 —— 用户填了也白填,于是「邀请码」看着像个摆设。
        inviteCode: inviteCode.value.trim() || null,
        allowInsecureHttp: allowInsecureHttp.value,
      })) as Record<string, unknown>;
      // 拿到的本机会话直接给协作 store 用 —— 用户在自己的机器上不该再登第二次。
      const token = String(r.token ?? "");
      if (token) await collab.adoptLocalSession(token);
      account.rememberEmail(email.value);
      await account.refresh();
      // 新后端只有本机准入 + 云端 enroll + 原子落库全通过才会返回；仍兼容老后端可能
      // 带回的半入网告警，升级过程不能把真实阻塞静默吞掉。
      const warn = String(r.memberWarn ?? "");
      if (warn) {
        err.value = warn;
        if (warn.includes("设备码")) showInvite.value = true;
      }
      emit("done", r);
    } else {
      await collab.loginWithEmailCode({
        email: email.value.trim(),
        code: code.value.trim(),
        inviteCode: inviteCode.value.trim(),
        allowInsecureHttp: allowInsecureHttp.value,
      });
      account.rememberEmail(email.value);
      await account.refresh();
      emit("done", {});
    }
  } catch (e) {
    const m = errMsg(e);
    // 进不了**别人**的机器 = 缺它的设备码。把那个输入框自动展开,别让用户猜。
    if (m.includes("设备码") || m.includes("邀请")) showInvite.value = true;
    if (m.includes("明文 HTTP") || m.includes("HTTPS")) showAdvanced.value = true;
    err.value = m;
  } finally {
    busy.value = false;
  }
}

function back() {
  step.value = "email";
  code.value = "";
  err.value = "";
}
</script>

<template>
  <div class="el-wrap">
    <div class="el-head">
      <span class="el-ic"><Mail :size="18" :stroke-width="2" /></span>
      <div>
        <h3 class="el-title">用邮箱登录</h3>
        <p class="el-lead">{{ props.lead }}</p>
      </div>
    </div>

    <!-- 第一步:邮箱 -->
    <template v-if="step === 'email'">
      <input
        v-model="email"
        class="el-inp"
        type="email"
        autocomplete="email"
        placeholder="你的邮箱,如 me@qq.com"
        @keyup.enter="sendCode"
      />
      <button class="el-cta" :disabled="busy || !emailOk" @click="sendCode">
        <LoaderCircle v-if="busy" :size="15" class="spin" />
        <Mail v-else :size="15" />
        发送验证码
      </button>
      <p class="el-note">
        第一次用这个邮箱?<b>直接发码就行</b> —— 验证过即自动开好账号,不用注册、不用设密码。
      </p>
    </template>

    <!-- 第二步:验证码 -->
    <template v-else>
      <p class="el-sent">
        验证码已发往 <b>{{ email }}</b>,10 分钟内有效。
      </p>
      <input
        v-model="code"
        class="el-inp el-code"
        inputmode="numeric"
        maxlength="6"
        autocomplete="one-time-code"
        placeholder="6 位验证码"
        @keyup.enter="submit"
      />
      <button class="el-cta" :disabled="busy || !codeOk" @click="submit">
        <LoaderCircle v-if="busy" :size="15" class="spin" />
        <ShieldCheck v-else :size="15" />
        登录
      </button>
      <div class="el-row">
        <button class="el-link" @click="back"><ArrowLeft :size="13" /> 换个邮箱</button>
        <button class="el-link" :disabled="cooldown > 0 || busy" @click="sendCode">
          {{ cooldown > 0 ? `${cooldown}s 后可重发` : "没收到?重发" }}
        </button>
      </div>

      <!-- 进**别人**的机器才要;自己的设备用不上,所以默认藏起来 -->
      <div v-if="showInvite" class="el-extra">
        <input v-model="inviteCode" class="el-inp" placeholder="那台机器的设备码,PLR1-… 或直接填 8 位码" />
        <p class="el-note">
          <b>你自己的设备不用填这个</b> —— 同一个邮箱登录就自动互联。
          只有要进<b>别人</b>的机器才填:让他打开<b>互联页第 ① 屏</b>,把那串<b>设备码</b>发给你。
          <b>全应用就这一种码</b>,他发给谁都是同一串。
        </p>
      </div>
      <button v-else class="el-link el-quiet" @click="showInvite = true">
        要进别人的机器?填它的设备码
      </button>
    </template>

    <p v-if="err" class="el-err">{{ err }}</p>

    <!-- 自建账号中心的人才需要。默认收起来 —— 地址不是用户该操心的东西 -->
    <div class="el-adv">
      <button class="el-link el-quiet" @click="showAdvanced = !showAdvanced">
        {{
          showAdvanced
            ? "收起"
            : isTauri && props.joinMesh
              ? "高级:自建了账号中心?"
              : "高级:当前账号中心仍是 HTTP?"
        }}
      </button>
      <input
        v-if="showAdvanced && isTauri && props.joinMesh"
        v-model="authorityUrl"
        class="el-inp el-sm"
        placeholder="账号中心地址（优先 https://；留空使用旧官方地址并要求确认）"
      />
      <label v-if="showAdvanced" class="el-risk">
        <input v-model="allowInsecureHttp" type="checkbox" />
        <span>
          {{
            isTauri && props.joinMesh
              ? "我确认这个旧账号中心使用明文 HTTP，邮箱、验证码和设备密钥可能被同链路窃取"
              : "我确认当前主机指定的账号中心使用明文 HTTP，邮箱、验证码和身份断言可能被同链路窃取"
          }}
        </span>
      </label>
    </div>
  </div>
</template>

<style scoped>
.el-wrap { display: flex; flex-direction: column; gap: 10px; }
.el-head { display: flex; gap: 11px; align-items: flex-start; }
.el-ic {
  flex: none; width: 34px; height: 34px; border-radius: 10px; display: grid; place-items: center;
  color: var(--acc, var(--primary)); background: color-mix(in srgb, var(--acc, var(--primary)) 14%, transparent);
  border: 1px solid color-mix(in srgb, var(--acc, var(--primary)) 26%, transparent);
}
.el-title { margin: 0; font-size: 16px; font-weight: 650; }
.el-lead { margin: 3px 0 0; font-size: 12.5px; color: var(--muted); line-height: 1.6; }
.el-inp {
  width: 100%; padding: 9px 12px; border-radius: 9px; font-size: 14px;
  color: var(--text); background: var(--panel);
  border: 1px solid var(--border); outline: none;
}
.el-inp::placeholder { color: var(--dim); opacity: 1; }
.el-inp:focus {
  border-color: color-mix(in srgb, var(--acc, var(--primary)) 60%, transparent);
  box-shadow: 0 0 0 3px color-mix(in srgb, var(--acc, var(--primary)) 12%, transparent);
}
.el-inp.el-sm { font-size: 12.5px; padding: 7px 10px; }
.el-code { letter-spacing: 6px; font-family: ui-monospace, Consolas, monospace; text-align: center; }
.el-cta {
  display: flex; align-items: center; justify-content: center; gap: 7px;
  padding: 10px 14px; border-radius: 10px; border: none; cursor: pointer;
  font-size: 14px; font-weight: 600; color: #fff;
  background: linear-gradient(135deg, var(--acc, var(--primary)), #8b5cf6);
}
.el-cta:disabled { opacity: .5; cursor: not-allowed; }
.el-row { display: flex; justify-content: space-between; align-items: center; }
.el-link {
  display: inline-flex; align-items: center; gap: 4px; background: none; border: none;
  color: var(--acc, var(--primary)); font-size: 12.5px; cursor: pointer; padding: 2px 0;
}
.el-link:disabled { color: var(--muted); cursor: default; }
.el-link.el-quiet { color: var(--muted); }
.el-note { margin: 0; font-size: 12px; color: var(--muted); line-height: 1.6; }
.el-sent { margin: 0; font-size: 12.5px; color: var(--dim, #98a2b8); }
.el-err {
  margin: 0; font-size: 12.5px; color: #ff9a9a; line-height: 1.6;
  background: rgba(239, 68, 68, .1); border-radius: 8px; padding: 7px 10px;
}
.el-extra { display: flex; flex-direction: column; gap: 6px; }
.el-adv { display: flex; flex-direction: column; gap: 6px; margin-top: 2px; }
.el-risk { display: flex; align-items: flex-start; gap: 7px; color: var(--vermilion, #e35d5d); font-size: 11.5px; line-height: 1.5; }
.el-risk input { flex: none; width: 14px; height: 14px; margin-top: 2px; accent-color: var(--vermilion, #e35d5d); }
.spin { animation: el-spin 1s linear infinite; }
@keyframes el-spin { to { transform: rotate(360deg); } }
</style>

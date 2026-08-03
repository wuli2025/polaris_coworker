<script setup lang="ts">
import { computed, nextTick, onMounted, onBeforeUnmount, ref, watch } from "vue";
import {
  messages,
  sending,
  sendMessage,
  cancel,
  newConversation,
  switchProject,
} from "../lib/chat";
import {
  projects,
  activeProject,
  loadProjects,
  matchProject,
  parseProjectIntent,
  dirName,
} from "../lib/projects";
import { renderMd } from "../lib/md";
import { invoke, upload } from "../lib/net";
import { VoiceRecorder, MAX_SECONDS } from "../lib/recorder";
import { hasCap } from "../lib/capabilities";
import { toast, toastErr } from "../lib/toast";
import { openPreview } from "../lib/preview";
import { go } from "../lib/nav";
import {
  list as listApps,
  matchApp,
  parseOpenIntent,
  requestOpen,
  type PubApp,
} from "../lib/apps";
import { onKeyboard } from "../lib/viewport";
import { hostName, user, displayName } from "../lib/auth";
import {
  chosenProviderId,
  chosenSkillIds,
  chosenProviderName,
  deepWork,
  providerState,
  skillState,
  loadProviders,
  loadSkills,
} from "../lib/pickers";
import TriggerSheet from "../components/TriggerSheet.vue";
import ChatDrawer from "../components/ChatDrawer.vue";
import SkillSheet from "../components/SkillSheet.vue";
import ModelSheet from "../components/ModelSheet.vue";
import Ico from "../components/Ico.vue";

const props = defineProps<{ wsOk: boolean }>();

// Kimi 式「一文件一预览」:本轮产物挑一个主可打开件当预览大按钮,其余收进「更多」里,
// 手机端小屏尤其不能一堆小文件铺满。ext 越靠前越该当主预览(html/演示 > pdf > 图 > 文档)。
const PREVIEW_RANK: Record<string, number> = {
  html: 0, htm: 0, svg: 2, pdf: 3,
  png: 4, jpg: 4, jpeg: 4, gif: 4, webp: 4,
  mp4: 5, mov: 5, webm: 5, pptx: 6, docx: 6, xlsx: 6,
  md: 7, markdown: 7, txt: 8, csv: 8,
};
function artRank(p: string): number {
  if (/polaris\.slides\.json$/i.test(p)) return -1;
  const n = p.split(/[\\/]/).pop() || p;
  const i = n.lastIndexOf(".");
  return i >= 0 ? PREVIEW_RANK[n.slice(i + 1).toLowerCase()] ?? 50 : 50;
}
/** 把本轮产物拆成 {preview: 主件, rest: 其余}(排除尾随 / 的文件夹产物) */
function artView(arts: string[]): { preview?: string; rest: string[] } {
  const files = arts.filter((a) => !a.endsWith("/"));
  if (!files.length) return { rest: [] };
  const sorted = [...files].sort((a, b) => artRank(a) - artRank(b));
  return { preview: sorted[0], rest: sorted.slice(1) };
}
// 展开「更多产物」的消息下标集合(默认折叠,不铺满)
const artsExpanded = ref<Set<number>>(new Set());
function toggleArts(i: number) {
  const s = new Set(artsExpanded.value);
  s.has(i) ? s.delete(i) : s.add(i);
  artsExpanded.value = s;
}
function baseName(p: string): string {
  return p.split(/[\\/]/).pop() || p;
}

/** 正文里被 md.ts 标成 data-path 的文件名 → 点一下就远程预览(事件代理,不给每个链接挂监听)。 */
function onMdClick(e: MouseEvent) {
  const hit = (e.target as HTMLElement | null)?.closest?.("[data-path]") as HTMLElement | null;
  if (!hit) return;
  const p = hit.getAttribute("data-path");
  if (!p) return;
  e.preventDefault();
  openPreview(p);
}

const draft = ref("");
const sheetOpen = ref(false);
const skillOpen = ref(false);
const modelOpen = ref(false);
const drawerOpen = ref(false);
const scroller = ref<HTMLElement | null>(null);
const ta = ref<HTMLTextAreaElement | null>(null);
const fileInput = ref<HTMLInputElement | null>(null);
const attachments = ref<string[]>([]);

// 静默预取模型/技能列表:chips 才知道该不该显示(老主机 unsupported 时隐藏)
let offKeyboard: (() => void) | null = null;
onMounted(() => {
  loadProviders();
  loadSkills();
  if (hasCap("voice")) probeVoice();
  // 键盘弹起时输入坞上抬 → 消息流同时变矮,不补一次滚动的话刚发的消息会被顶出视野。
  offKeyboard = onKeyboard(() => scrollDown());
  // 按住说话时被切后台/来电 → 松手事件收不到了,得主动收手,
  // 否则麦克风一直开着(状态栏挂录音图标),回来还会把这段没打算说的话发去识别。
  document.addEventListener("visibilitychange", onHidden);
});
function onHidden() {
  if (document.visibilityState !== "hidden") return;
  pressing = false; // 切后台时 pointerup 多半不会来了
  if (recState.value === "rec") {
    stopLevelTimer();
    recorder.cancel();
    recState.value = "idle";
    recLevel.value = 0;
    willCancel.value = false;
  }
}
onBeforeUnmount(() => {
  offKeyboard?.();
  document.removeEventListener("visibilitychange", onHidden);
  // 离开页面时别把麦克风占着不放(安卓状态栏会一直挂着录音图标)
  stopLevelTimer();
  if (recState.value === "rec") recorder.cancel();
});

async function scrollDown() {
  await nextTick();
  const el = scroller.value;
  if (el) el.scrollTop = el.scrollHeight;
}
watch(() => messages.length, scrollDown);
watch(
  () => messages[messages.length - 1]?.text,
  scrollDown
);

/**
 * 「打开××项目」先在本地截:切项目是选一个工作目录,不必劳驾大模型 —— 秒进,零轮次。
 * 切完之后这条对话里说的话,主机都在该项目绑定的文件夹里执行。
 * 返回 true = 已处理,别再发给模型。判定顺序在 tryOpenApp 之前(说了「项目」二字的归这里)。
 */
async function tryOpenProject(t: string): Promise<boolean> {
  const intent = parseProjectIntent(t);
  if (!intent) return false;
  if (intent.kind === "list") {
    go("work");
    return true;
  }
  let list = projects.value;
  try {
    // 清单可能是冷的(电脑上刚建的项目);拉一次再匹配,失败就用手上这份。
    list = (await loadProjects()) ?? list;
  } catch {
    /* 老主机/断网:用缓存的清单匹配,匹配不上照常发给模型 */
  }
  const hit = matchProject(list, intent.name);
  if (hit) {
    await switchProject(hit);
    toast(hit.work_dir ? `已进入「${hit.name}」· ${dirName(hit.work_dir)}` : `已进入「${hit.name}」`, "ok");
    return true;
  }
  if (intent.explicit) {
    // 明说了要开「××项目」却没这个项目 → 带去项目页自己挑/新建,比让模型编一段强
    toast(list.length ? `电脑上没有叫「${intent.name}」的项目` : "电脑上还没有项目");
    go("work");
    return true;
  }
  return false; // 只是「切到××」而对不上项目 → 当普通消息发,绝不吞用户的话
}

/**
 * 「打开××」先在本地截:开电脑上的应用不需要劳驾大模型 —— 秒进,零轮次。
 * 语音说的也走这里(转写完同样从 send 出去)。返回 true = 已处理,别再发给模型。
 */
async function tryOpenApp(t: string): Promise<boolean> {
  const intent = parseOpenIntent(t);
  if (!intent) return false;
  if (intent.kind === "list") {
    go("apps");
    return true;
  }
  let apps: PubApp[] = [];
  try {
    apps = (await listApps()) ?? [];
  } catch {
    return false; // 主机不支持/临时抖动 → 当普通消息发,别拦
  }
  const hit = matchApp(apps, intent.name);
  if (hit) {
    toast(`正在打开「${hit.name}」`, "ok");
    requestOpen(hit.slug);
    return true;
  }
  if (intent.explicit) {
    // 明说了要开「××应用」却没这个应用 → 带去应用页自己挑,比让模型编一段强
    toast(apps.length ? `没有叫「${intent.name}」的应用,看看已发布的` : "电脑还没发布任何应用");
    go("apps");
    return true;
  }
  return false;
}

async function send() {
  const t = draft.value.trim();
  if (!t) return;
  // 先项目后应用:「打开××项目」明确点了项目,不能被「打开××(应用)」抢走。
  if ((await tryOpenProject(t)) || (await tryOpenApp(t))) {
    draft.value = "";
    if (ta.value) ta.value.style.height = "auto";
    return;
  }
  draft.value = "";
  if (ta.value) ta.value.style.height = "auto";
  const atts = attachments.value.slice();
  attachments.value = [];
  await sendMessage(t, {
    attachments: atts.length ? atts : undefined,
    workMode: deepWork.value ? "work" : undefined,
    providerId: chosenProviderId.value,
    skillIds: chosenSkillIds.value.length ? chosenSkillIds.value.slice() : undefined,
  });
}

function growTa(el: HTMLTextAreaElement | null) {
  if (!el) return;
  el.style.height = "auto";
  el.style.height = Math.min(el.scrollHeight, 140) + "px";
}
function autoGrow(e: Event) {
  growTa(e.target as HTMLTextAreaElement);
}

// ── 语音输入:按住说话 → 录 16k WAV → 主机侧火山识别 → 文字填进输入框 ──
// 上滑取消是国内 App 的肌肉记忆(微信/豆包都这样),不做的话手滑了只能把错字删掉。
const recorder = new VoiceRecorder();
const recState = ref<"idle" | "rec" | "asr">("idle");
const recLevel = ref(0);
/** 距离硬顶还剩几秒(只在最后 10 秒提示,免得平时数字乱跳分心)。 */
const recLeft = ref(MAX_SECONDS);
const willCancel = ref(false);
// 主机支不支持云端转写。手机 APK 与主机各自发版,连到没升级的老主机时,
// 不探一下就会「按住说完 45 秒才被告知命令不支持」。探法很轻:发一个空音频,
// 新主机回「没有收到音频数据」(说明命令在),老主机回「…暂不支持」→ 直接把钮藏掉。
const voiceSupported = ref(true);
const voiceOn = computed(() => hasCap("voice") && voiceSupported.value);

async function probeVoice() {
  try {
    await invoke("voice_transcribe_audio", { audio: "", format: "wav" });
  } catch (e) {
    const msg = (e as Error)?.message ?? "";
    if (msg.includes("不支持") || msg.includes("未知命令")) voiceSupported.value = false;
  }
}
const CANCEL_DIST = 80; // 上滑超过这个距离(px)即取消
let pressY = 0;
let levelTimer: ReturnType<typeof setInterval> | null = null;
/** recorder.start() 在途(含系统权限框那几百毫秒)。 */
let starting = false;
/** 手指还按在麦克风上吗 —— 启动窗口里松手的唯一判据。 */
let pressing = false;

function stopLevelTimer() {
  if (levelTimer) {
    clearInterval(levelTimer);
    levelTimer = null;
  }
}

async function micDown(e: PointerEvent) {
  if (recState.value !== "idle" || starting || sending.value) return;
  pressY = e.clientY;
  willCancel.value = false;
  // 捕获指针:手指滑出按钮范围后仍能收到 up/move,否则松手事件会丢、录音停不下来。
  (e.currentTarget as HTMLElement)?.setPointerCapture?.(e.pointerId);
  // starting/pressing:recorder.start() 是异步的,**首次使用还要等系统权限框**,
  // 这期间用户手指早抬起来了 —— 那一发 pointerup 会因为 recState 还是 idle 被守卫吞掉,
  // 结果授权一过就自己开始录音、浮层盖屏,人却没在按。所以要记「手指还按着吗」。
  starting = true;
  pressing = true;
  try {
    await recorder.start();
    if (!pressing) {
      // 手指在授权/启动期间已经松开(或被 pointercancel 打断)→ 立刻收手,别自己录起来。
      recorder.cancel();
      return;
    }
    recState.value = "rec";
    levelTimer = setInterval(() => {
      recLevel.value = recorder.level;
      recLeft.value = Math.max(0, Math.ceil(MAX_SECONDS - recorder.elapsed));
      // 到顶自动收尾:再录下去 base64 会顶破 /api/invoke 的 2MB body 上限,
      // 与其让用户说满一分钟吃个 413,不如替他把话收住并直接送去识别。
      if (recorder.elapsed >= MAX_SECONDS) {
        willCancel.value = false;
        void micUp();
      }
    }, 80);
    navigator.vibrate?.(12);
  } catch (err) {
    toastErr(err);
    recState.value = "idle";
  } finally {
    starting = false;
  }
}

function micMove(e: PointerEvent) {
  if (recState.value !== "rec") return;
  willCancel.value = pressY - e.clientY > CANCEL_DIST;
}

/** 松手 = 发送。 */
function micRelease() {
  pressing = false;
  void micUp();
}
/** 被系统打断(来电 / 系统手势接管 / 按钮被移除)= **取消**,不是发送 ——
 *  照微信/豆包的习惯,意外中断绝不能把半句话替用户发出去。 */
function micAbort() {
  pressing = false;
  if (recState.value !== "rec") return;
  willCancel.value = true;
  void micUp();
}

async function micUp() {
  if (recState.value !== "rec") return;
  stopLevelTimer();
  recLevel.value = 0;
  recLeft.value = MAX_SECONDS;
  if (willCancel.value) {
    recorder.cancel();
    recState.value = "idle";
    toast("已取消");
    return;
  }
  recState.value = "asr";
  try {
    const out = await recorder.stop();
    if (!out) {
      toast("说得太短啦,按住多说两句");
      return;
    }
    const r = await invoke<{ text?: string; raw?: string }>("voice_transcribe_audio", {
      audio: out.base64,
      format: "wav",
    });
    const t = (r?.text || r?.raw || "").trim();
    if (!t) {
      toast("没听清,再说一次试试");
      return;
    }
    // 追加而不是覆盖:用户可能先打了一半字再补一句话。
    draft.value = draft.value.trim() ? `${draft.value.trimEnd()} ${t}` : t;
    await nextTick();
    growTa(ta.value);
    ta.value?.focus();
  } catch (err) {
    toastErr(err);
  } finally {
    recState.value = "idle";
  }
}

async function pickFile() {
  fileInput.value?.click();
}
async function onFiles(e: Event) {
  const files = (e.target as HTMLInputElement).files;
  if (!files || !files.length) return;
  try {
    const up = await upload(files);
    attachments.value.push(...up.map((f) => f.path));
    toast(`已附上 ${up.length} 个文件`, "ok");
  } catch (e) {
    toastErr(e);
  } finally {
    (e.target as HTMLInputElement).value = "";
  }
}

// 建议:Kimi/豆包式**浮在输入框上方**的一条横滑胶囊(不再是空状态里的两列大卡片,
// 因为大卡片只在空对话时有用,而横滑条聊到一半也能随手点)。点一下把模板填进输入框。
interface Feature {
  icon: string;
  label: string;
  template: string;
}
const FEATURES: Feature[] = [
  { icon: "deck", label: "做 PPT", template: "帮我做一份 PPT,主题是:" },
  { icon: "globe", label: "做网页", template: "帮我做一个 HTML 网页,要求:" },
  { icon: "doc", label: "写文档", template: "帮我写一份文档,主题是:" },
  { icon: "chart", label: "数据分析", template: "帮我分析这份数据,重点看:" },
  { icon: "stack", label: "总结文件", template: "帮我总结这份文件的要点" },
  { icon: "calendar", label: "写周报", template: "帮我生成一份周报,本周做了:" },
];
function useFeature(f: Feature) {
  draft.value = f.template;
  nextTick(() => {
    ta.value?.focus();
    // 光标停在模板末尾,接着补细节
    ta.value?.setSelectionRange(draft.value.length, draft.value.length);
  });
}

const modelLabel = computed(() => chosenProviderName() || "Auto");
const skillLabel = computed(() =>
  chosenSkillIds.value.length ? `技能 ${chosenSkillIds.value.length}` : "技能"
);
// 建议条:没在生成、且还没开始打字时才占位(打字时让位给输入,避免抢空间)
const showSuggest = computed(() => !sending.value && !draft.value.trim());
</script>

<template>
  <div class="chat">
    <!-- 顶栏:三块悬浮琉璃(左抽屉 / 中主机·模型 / 右新对话),不压横线,背景透出极光 -->
    <header class="bar">
      <button class="orb-btn" title="展开" @click="drawerOpen = true">
        <Ico name="menu" :size="19" />
      </button>
      <button class="hpill" @click="modelOpen = true">
        <span class="hp-host">{{ hostName || "对话" }}</span>
        <span class="hp-model">{{ modelLabel }}</span>
        <span class="hp-dot" :class="{ live: wsOk }" :title="wsOk ? '已连接' : '连接中'"></span>
      </button>
      <button class="orb-btn" title="新对话" @click="newConversation">
        <Ico name="compose" :size="18" />
      </button>
    </header>

    <!-- 在哪个项目里干活:选了才显示,点一下换。不选时行为同旧版(主机默认目录) -->
    <button v-if="activeProject" class="projbar" @click="go('work')">
      <Ico name="folder" :size="13" />
      <span class="pb-name">{{ activeProject.name }}</span>
      <span v-if="activeProject.work_dir" class="pb-dir">{{ dirName(activeProject.work_dir) }}</span>
      <Ico name="swap" :size="12" class="pb-sw" />
    </button>

    <div ref="scroller" class="stream">
      <div v-if="!messages.length" class="empty">
        <div class="orb"><i></i></div>
        <div class="hi">
          嗨 {{ displayName(user) }},<br />今天想做点什么
        </div>
        <div class="hint faint">
          {{
            activeProject
              ? `现在在「${activeProject.name}」里，说的活都在电脑的这个文件夹里做`
              : "说一句「打开 ×× 项目」，就能进电脑上的项目干活"
          }}
        </div>
      </div>

      <div v-for="(m, i) in messages" :key="i" class="row" :class="m.role">
        <div v-if="m.role === 'user'" class="bubble user">{{ m.text }}</div>
        <div v-else-if="m.role === 'tool'" class="tool-chip">
          <Ico name="tool" :size="13" /> {{ m.text }}
        </div>
        <div v-else-if="m.role === 'error'" class="bubble err">{{ m.text }}</div>
        <div v-else class="bubble asst">
          <div class="md" @click="onMdClick" v-html="renderMd(m.text)"></div>
          <div v-if="m.artifacts?.length" class="arts">
            <!-- 主预览大按钮:本轮最该「打开看」的那个件 -->
            <button
              v-if="artView(m.artifacts).preview"
              class="art-preview"
              @click="openPreview(artView(m.artifacts).preview!)"
            >
              <Ico name="eye" :size="16" />
              <span class="ap-name">{{ baseName(artView(m.artifacts).preview!) }}</span>
              <span class="ap-tag">预览</span>
            </button>
            <!-- 其余产物:默认折叠成一行「共 N 个」, 点开才列出, 不铺满小屏 -->
            <template v-if="artView(m.artifacts).rest.length">
              <button class="art-more" @click="toggleArts(i)">
                {{ artsExpanded.has(i) ? "收起" : `本轮共 ${m.artifacts.length} 个文件` }}
                <Ico name="chevron" :size="12" :class="{ up: artsExpanded.has(i) }" />
              </button>
              <template v-if="artsExpanded.has(i)">
                <button
                  v-for="p in artView(m.artifacts).rest"
                  :key="p"
                  class="art"
                  @click="openPreview(p)"
                >
                  <Ico name="clip" :size="13" /> {{ baseName(p) }}
                </button>
              </template>
            </template>
          </div>
        </div>
      </div>
      <div v-if="sending" class="typing"><i></i><i></i><i></i></div>
    </div>

    <!-- 底部:建议横滑条 + 输入卡片(都贴着屏底浮起来) -->
    <div class="dock">
      <transition name="lift">
        <div v-if="showSuggest" class="suggest">
          <button v-for="f in FEATURES" :key="f.label" class="sg" @click="useFeature(f)">
            <Ico :name="f.icon" :size="16" />
            {{ f.label }}
          </button>
        </div>
      </transition>

      <div class="card">
        <div v-if="attachments.length" class="atts">
          <span v-for="(a, i) in attachments" :key="i" class="att-chip">
            <Ico name="clip" :size="12" />
            {{ a.split(/[\\/]/).pop() }}
            <button @click="attachments.splice(i, 1)"><Ico name="close" :size="12" /></button>
          </span>
        </div>
        <textarea
          ref="ta"
          v-model="draft"
          class="ta"
          rows="1"
          placeholder="尽管问,带图也行"
          @input="autoGrow"
          @focus="scrollDown"
          @keydown.enter.exact.prevent="send"
        ></textarea>
        <div class="tools">
          <button class="round" title="更多" @click="sheetOpen = true">
            <Ico name="plus" :size="19" />
          </button>
          <div class="chips">
            <button
              v-if="skillState !== 'unsupported'"
              class="chip"
              :class="{ on: chosenSkillIds.length }"
              @click="skillOpen = true"
            >
              <Ico name="spark" :size="14" /> {{ skillLabel }}
            </button>
            <button
              v-if="providerState !== 'unsupported'"
              class="chip"
              :class="{ on: chosenProviderId !== 'auto' }"
              @click="modelOpen = true"
            >
              <Ico name="stack" :size="14" /> {{ modelLabel }}
            </button>
            <button class="chip" :class="{ on: deepWork }" @click="deepWork = !deepWork">
              <Ico name="brain" :size="14" /> 深度工作
            </button>
          </div>
          <!-- 按住说话:pointer 事件 + setPointerCapture,手指滑出按钮也收得到松手。
               **发送中只 disable、不 v-if 移除** —— 元素一旦从 DOM 拿掉,pointer capture
               跟着销毁,松手事件永远到不了,录音和浮层就会一直挂到 45 秒硬顶才自愈。 -->
          <button
            v-if="voiceOn"
            class="round mic"
            :class="{ rec: recState === 'rec', busy: recState === 'asr' }"
            :disabled="recState === 'asr' || (sending && recState === 'idle')"
            title="按住说话"
            @pointerdown.prevent="micDown"
            @pointermove="micMove"
            @pointerup="micRelease"
            @pointercancel="micAbort"
            @contextmenu.prevent
          >
            <Ico name="mic" :size="18" />
          </button>
          <button v-if="sending" class="go stop" @click="cancel">
            <Ico name="stop" :size="18" :stroke-width="2" />
          </button>
          <button v-else class="go" :disabled="!draft.trim()" @click="send">
            <Ico name="send" :size="18" :stroke-width="2" />
          </button>
        </div>
      </div>
      <div class="foot">内容由 AI 生成</div>
    </div>

    <!-- 录音浮层:说话时盖住半屏给足反馈(声波 + 上滑取消提示),识别时转成「识别中」 -->
    <transition name="fade">
      <div v-if="recState !== 'idle'" class="rec-veil">
        <div class="rec-card" :class="{ cancel: willCancel, busy: recState === 'asr' }">
          <div class="wave">
            <i
              v-for="n in 5"
              :key="n"
              :style="{
                transform: `scaleY(${(0.22 + recLevel * (n % 2 ? 1.7 : 1.15)).toFixed(2)})`,
              }"
            ></i>
          </div>
          <p class="rec-tip">
            {{
              recState === "asr"
                ? "识别中…"
                : willCancel
                  ? "松手取消"
                  : recLeft <= 10
                    ? `还能说 ${recLeft} 秒`
                    : "松开发送 · 上滑取消"
            }}
          </p>
        </div>
      </div>
    </transition>

    <input ref="fileInput" type="file" multiple hidden @change="onFiles" />
    <TriggerSheet v-model="sheetOpen" @attach="pickFile" />
    <SkillSheet v-model="skillOpen" />
    <ModelSheet v-model="modelOpen" />
    <ChatDrawer v-model="drawerOpen" :ws-ok="props.wsOk" />
  </div>
</template>

<style scoped>
.chat {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-height: 0;
}

/* ── 顶栏:悬浮琉璃三件套 ──
   上内边距 = 真实安全区 + 10px 呼吸位:安卓 15 起 WebView 铺到状态栏/挖孔底下,
   safe-top 由原生 MainActivity 注入(见 lib/viewport.ts),再压一档间距,
   顶栏就整体落在刘海下方一点点,而不是贴着甚至压住摄像头。 */
.bar {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: calc(10px + var(--safe-top)) 12px 6px;
  background: transparent;
}
/* 顶栏三件是真·悬浮层,允许 blur;它们的内侧高光走 .specular::before */
.orb-btn,
.hpill {
  position: relative;
  background: var(--glass);
  -webkit-backdrop-filter: var(--blur-sm);
  backdrop-filter: var(--blur-sm);
  border: 1px solid var(--line);
  border-top-color: var(--line-hi);
  border-bottom-color: var(--line-lo);
  box-shadow: var(--shadow-sm);
  color: var(--text-dim);
  overflow: hidden;
  transition: transform 0.13s cubic-bezier(0.32, 0.72, 0, 1), filter 0.13s ease;
  touch-action: manipulation;
}
.orb-btn:active,
.hpill:active {
  transform: scale(0.94);
  filter: brightness(0.9);
}
.orb-btn::before,
.hpill::before {
  content: "";
  position: absolute;
  inset: 0;
  border-radius: inherit;
  pointer-events: none;
  background:
    radial-gradient(80% 46% at 20% 0%, rgba(255, 255, 255, 0.22), transparent 60%),
    linear-gradient(180deg, rgba(255, 255, 255, 0.08), transparent 45%);
}
.orb-btn {
  width: 40px;
  height: 40px;
  border-radius: 50%;
  display: grid;
  place-items: center;
  flex-shrink: 0;
}
.hpill {
  flex: 1;
  min-width: 0;
  height: 40px;
  border-radius: 999px;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 7px;
  padding: 0 16px;
}
.hpill:active {
  transform: scale(0.98);
}
.hp-host {
  font-weight: 600;
  font-size: 15px;
  color: var(--text);
  max-width: 46%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.hp-model {
  font-size: 13.5px;
  color: var(--text-faint);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.hp-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--text-faint);
  flex-shrink: 0;
}
.hp-dot.live {
  background: var(--ok);
  box-shadow: 0 0 7px var(--ok);
}

/* ── 消息流 ── */
.stream {
  flex: 1;
  overflow-y: auto;
  padding: 10px 14px 4px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.empty {
  margin: auto 0;
  padding: 0 4px 12vh;
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
}
/* 水晶球:不是「蓝色吉祥物」,是一颗真玻璃珠 ——
   本体半透明只做 backdrop 折射(blur+饱和+提亮),再叠镜面高光 / 边缘环光 / 底部焦散。 */
.orb {
  position: relative;
  width: 96px;
  height: 96px;
  border-radius: 50%;
  background:
    radial-gradient(58% 52% at 32% 26%, rgba(255, 255, 255, 0.5), transparent 62%),
    linear-gradient(150deg, rgba(124, 155, 255, 0.5), rgba(180, 140, 255, 0.34));
  -webkit-backdrop-filter: saturate(220%) brightness(1.25) blur(6px);
  backdrop-filter: saturate(220%) brightness(1.25) blur(6px);
  box-shadow:
    inset 0 2px 2px rgba(255, 255, 255, 0.75),
    inset 0 -14px 26px rgba(60, 40, 140, 0.36),
    inset 0 0 0 1px rgba(255, 255, 255, 0.28),
    0 18px 40px rgba(96, 116, 255, 0.32);
  animation: float 6.5s ease-in-out infinite;
}
/* 底部焦散:光穿过玻璃球在下缘聚出来的那道亮弧 */
.orb i {
  position: absolute;
  inset: 0;
  border-radius: 50%;
  background:
    radial-gradient(38% 22% at 66% 82%, rgba(255, 255, 255, 0.55), transparent 70%),
    radial-gradient(18% 12% at 30% 22%, rgba(255, 255, 255, 0.95), transparent 72%);
}
@keyframes float {
  0%, 100% { transform: translateY(0) }
  50% { transform: translateY(-9px) }
}
@media (prefers-reduced-motion: reduce) {
  .orb { animation: none }
}
.hi {
  margin-top: 24px;
  font-size: 22px;
  font-weight: 500;
  letter-spacing: 0.2px;
  line-height: 1.5;
  color: var(--text);
}
.hint {
  margin-top: 10px;
  max-width: 17em;
  font-size: 12.5px;
  line-height: 1.7;
}
/* 项目条:顶栏与消息流之间的一道细提示,只在选了项目时出现 */
.projbar {
  display: flex;
  align-items: center;
  gap: 6px;
  align-self: center;
  max-width: calc(100% - 28px);
  margin: 2px auto 0;
  padding: 4px 11px;
  border-radius: 999px;
  background: var(--glass);
  -webkit-backdrop-filter: var(--blur-sm);
  backdrop-filter: var(--blur-sm);
  border: 1px solid var(--line);
  border-top-color: var(--line-hi);
  color: var(--text-dim);
  font-size: 11.5px;
  flex-shrink: 0;
  touch-action: manipulation;
}
.projbar:active {
  filter: brightness(0.9);
}
.pb-name {
  font-weight: 600;
  color: var(--text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.pb-dir {
  color: var(--text-faint);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.pb-dir::before {
  content: "· ";
}
.pb-sw {
  flex-shrink: 0;
  opacity: 0.7;
}
.row {
  display: flex;
}
.row.user {
  justify-content: flex-end;
}
.bubble {
  position: relative;
  max-width: 84%;
  padding: 10px 14px;
  border-radius: 18px;
  word-break: break-word;
}
/* 气泡上沿的内高光:同一套光学(光从斜上方进玻璃),比单纯提高底色不透明度便宜也更透 */
.bubble.asst::before,
.bubble.user::before {
  content: "";
  position: absolute;
  inset: 0;
  border-radius: inherit;
  pointer-events: none;
  background: linear-gradient(180deg, rgba(255, 255, 255, 0.1), transparent 42%);
}
.bubble > * {
  position: relative;
}
/* 气泡**不做 backdrop-filter**:一屏几十条 = 几十个合成层,Android WebView 直接掉帧,
   而且玻璃叠玻璃越叠越灰。改用稍厚的半透明 + 上缘高光/下缘暗边,一样有厚度。 */
.bubble.user {
  background: var(--user-bubble);
  border: 1px solid var(--line);
  border-top-color: var(--line-hi);
  border-bottom-color: var(--line-lo);
  border-bottom-right-radius: 6px;
  box-shadow: var(--shadow-sm);
}
.bubble.asst {
  background: var(--glass-solid);
  border: 1px solid var(--line);
  border-top-color: var(--line-hi);
  border-bottom-color: var(--line-lo);
  border-bottom-left-radius: 6px;
  box-shadow: var(--shadow-sm);
}
.bubble.err {
  background: transparent;
  border: 1px solid var(--danger);
  color: var(--danger);
}
.tool-chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  color: var(--text-dim);
  background: var(--glass2);
  border: 1px solid var(--line);
  border-top-color: var(--line-hi);
  padding: 6px 12px;
  border-radius: 20px;
}
.arts {
  margin-top: 8px;
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}
/* 主预览大按钮:整行醒目, 一眼看到本轮主产物 */
.art-preview {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 9px 12px;
  border: 1px solid var(--line);
  border-radius: 13px;
  background: var(--accent-soft);
  color: var(--text);
  text-align: left;
}
.art-preview .ap-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 13.5px;
  font-weight: 600;
}
.art-preview .ap-tag {
  flex-shrink: 0;
  font-size: 11px;
  font-weight: 600;
  padding: 2px 8px;
  border-radius: 7px;
  background: var(--accent);
  color: #fff;
}
/* 「共 N 个」折叠条 */
.art-more,
.art {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: 12.5px;
  padding: 5px 11px;
  border: 1px solid var(--line);
  border-top-color: var(--line-hi);
  border-radius: 999px;
  background: var(--glass2);
  color: var(--text-dim);
}
.art-more .ico.up {
  transform: rotate(180deg);
}
/* 三点呼吸:比「正在思考…」安静 */
.typing {
  display: flex;
  gap: 5px;
  padding: 6px 4px;
}
.typing i {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--text-faint);
  animation: blink 1.3s ease-in-out infinite;
}
.typing i:nth-child(2) { animation-delay: 0.18s }
.typing i:nth-child(3) { animation-delay: 0.36s }
@keyframes blink {
  0%, 100% { opacity: 0.25; transform: translateY(0) }
  40% { opacity: 1; transform: translateY(-3px) }
}

/* ── 底坞:建议条 + 输入卡片 ──
   底内边距把键盘高度 --kb 也算进去:输入坞被顶起、消息流(flex:1)相应缩短,
   于是点开输入框时输入卡浮在输入法**上方**而不是被盖住。
   --safe-bottom 是导航栏,--kb 是键盘净高(原生已减掉导航栏,两者相加正好等于遮挡总高)。 */
.dock {
  padding: 0 10px calc(4px + var(--safe-bottom) + var(--kb));
  transition: padding-bottom 0.18s cubic-bezier(0.32, 0.72, 0, 1);
}
.suggest {
  display: flex;
  gap: 8px;
  overflow-x: auto;
  scrollbar-width: none;
  padding: 6px 2px 9px;
  -webkit-mask-image: linear-gradient(90deg, #000 94%, transparent);
  mask-image: linear-gradient(90deg, #000 94%, transparent);
}
.suggest::-webkit-scrollbar {
  display: none;
}
/* 建议胶囊:6 个一排,**不给 blur**(6 个合成层不值),用厚一档的半透明压底 */
.sg {
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 8px 14px;
  border-radius: 999px;
  font-size: 13.5px;
  color: var(--text-dim);
  background: var(--glass-solid);
  border: 1px solid var(--line);
  border-top-color: var(--line-hi);
  border-bottom-color: var(--line-lo);
  box-shadow: var(--shadow-sm);
  white-space: nowrap;
  transition: transform 0.13s cubic-bezier(0.32, 0.72, 0, 1), filter 0.13s ease;
  touch-action: manipulation;
}
.sg:active {
  transform: scale(0.94);
  filter: brightness(0.9);
  color: var(--accent);
}
.lift-enter-active,
.lift-leave-active {
  transition: opacity 0.18s ease, transform 0.18s ease;
}
.lift-enter-from,
.lift-leave-to {
  opacity: 0;
  transform: translateY(8px);
}
/* 输入卡:整个 App 最厚的一片玻璃,该有的都给足 */
.card {
  position: relative;
  background: var(--glass);
  -webkit-backdrop-filter: var(--blur);
  backdrop-filter: var(--blur);
  border: 1px solid var(--line);
  border-top-color: var(--line-hi);
  border-bottom-color: var(--line-lo);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-lg);
  padding: 10px 12px 9px;
}
.card::before {
  content: "";
  position: absolute;
  inset: 0;
  border-radius: inherit;
  pointer-events: none;
  background:
    radial-gradient(70% 40% at 14% 0%, rgba(255, 255, 255, 0.2), transparent 58%),
    linear-gradient(180deg, rgba(255, 255, 255, 0.07), transparent 34%);
}
.card > * {
  position: relative;
}
.atts {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-bottom: 6px;
}
.att-chip {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: 12px;
  background: var(--glass2);
  border-radius: 9px;
  padding: 4px 8px;
}
.att-chip button {
  color: var(--text-faint);
  display: flex;
}
.ta {
  display: block;
  width: 100%;
  resize: none;
  max-height: 140px;
  padding: 5px 6px 9px;
  background: transparent;
  border: none;
  outline: none;
  line-height: 1.45;
  font-size: 15.5px;
}
.ta::placeholder {
  color: var(--text-faint);
}
.tools {
  display: flex;
  align-items: center;
  gap: 8px;
}
.round {
  width: 34px;
  height: 34px;
  border-radius: 50%;
  background: var(--glass2);
  border: 1px solid var(--line);
  border-top-color: var(--line-hi);
  color: var(--text-dim);
  display: grid;
  place-items: center;
  flex-shrink: 0;
  transition: transform 0.13s cubic-bezier(0.32, 0.72, 0, 1), filter 0.13s ease;
}
.round:active,
.chip:active,
.go:active {
  transform: scale(0.92);
  filter: brightness(0.9);
}
.chips {
  flex: 1;
  display: flex;
  gap: 6px;
  overflow-x: auto;
  scrollbar-width: none;
  min-width: 0;
}
.chips::-webkit-scrollbar {
  display: none;
}
.chip {
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: 12.5px;
  padding: 6px 11px;
  border-radius: 999px;
  background: var(--glass2);
  border: 1px solid var(--line);
  border-top-color: var(--line-hi);
  color: var(--text-dim);
  transition: background 0.15s, color 0.15s, border-color 0.15s,
    transform 0.13s cubic-bezier(0.32, 0.72, 0, 1), filter 0.13s ease;
  white-space: nowrap;
  touch-action: manipulation;
}
.chip.on {
  background: var(--accent-soft);
  border-color: var(--accent);
  color: var(--accent);
}
.go {
  width: 34px;
  height: 34px;
  border-radius: 50%;
  background: linear-gradient(140deg, var(--accent), var(--accent-2));
  box-shadow:
    inset 0 1px 0 rgba(255, 255, 255, 0.4),
    0 4px 12px rgba(108, 140, 255, 0.42);
  color: #fff;
  display: grid;
  place-items: center;
  flex-shrink: 0;
  transition: transform 0.13s cubic-bezier(0.32, 0.72, 0, 1), filter 0.13s ease;
}
.go:disabled {
  background: var(--glass2);
  color: var(--text-faint);
  box-shadow: none;
}
.go.stop {
  background: var(--danger);
  box-shadow: none;
}
/* 麦克风:平时和「更多」同款小圆钮,按住变红并轻微放大 —— 手指盖住按钮时,
   放大后露出的一圈是「确实在录」的唯一视觉证据。 */
.mic.rec {
  background: var(--danger);
  border-color: transparent;
  color: #fff;
  transform: scale(1.12);
  box-shadow: 0 0 0 6px rgba(255, 107, 107, 0.18);
}
.mic.busy {
  opacity: 0.6;
}

/* ── 录音浮层 ── */
.rec-veil {
  position: fixed;
  inset: 0;
  z-index: 220;
  display: flex;
  align-items: center;
  justify-content: center;
  /* scrim 不做 brightness/blur:抽屉那次踩过,盖一层滤镜整屏必发灰 */
  background: var(--scrim);
  pointer-events: none;
}
.rec-card {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 16px;
  padding: 26px 34px;
  border-radius: var(--radius-lg);
  background: var(--glass);
  -webkit-backdrop-filter: var(--blur);
  backdrop-filter: var(--blur);
  border: 1px solid var(--line);
  border-top-color: var(--line-hi);
  border-bottom-color: var(--line-lo);
  box-shadow: var(--shadow-lg);
}
.wave {
  display: flex;
  align-items: center;
  gap: 6px;
  height: 46px;
}
/* 五根柱子按实时音量伸缩:transform 走合成层,不触发布局,录音时也不掉帧 */
.wave i {
  width: 5px;
  height: 46px;
  border-radius: 3px;
  background: linear-gradient(180deg, var(--accent), var(--accent-2));
  transform: scaleY(0.22);
  transition: transform 0.08s linear;
}
.rec-card.cancel .wave i {
  background: var(--danger);
}
.rec-card.busy .wave i {
  animation: blink 1.1s ease-in-out infinite;
}
.rec-card.busy .wave i:nth-child(2) { animation-delay: 0.12s }
.rec-card.busy .wave i:nth-child(3) { animation-delay: 0.24s }
.rec-card.busy .wave i:nth-child(4) { animation-delay: 0.36s }
.rec-card.busy .wave i:nth-child(5) { animation-delay: 0.48s }
.rec-tip {
  margin: 0;
  font-size: 13.5px;
  color: var(--text-dim);
}
.rec-card.cancel .rec-tip {
  color: var(--danger);
}
.foot {
  text-align: center;
  font-size: 11px;
  color: var(--text-faint);
  opacity: 0.7;
  padding: 7px 0 2px;
}
</style>

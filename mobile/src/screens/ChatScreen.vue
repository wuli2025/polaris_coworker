<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { messages, sending, sendMessage, cancel, newConversation } from "../lib/chat";
import { renderMd } from "../lib/md";
import { upload } from "../lib/net";
import { toast, toastErr } from "../lib/toast";
import { openPreview } from "../lib/preview";
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
onMounted(() => {
  loadProviders();
  loadSkills();
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

async function send() {
  const t = draft.value.trim();
  if (!t) return;
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

function autoGrow(e: Event) {
  const el = e.target as HTMLTextAreaElement;
  el.style.height = "auto";
  el.style.height = Math.min(el.scrollHeight, 140) + "px";
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

    <div ref="scroller" class="stream">
      <div v-if="!messages.length" class="empty">
        <div class="orb"><i></i></div>
        <div class="hi">
          嗨 {{ displayName(user) }},<br />今天想做点什么
        </div>
      </div>

      <div v-for="(m, i) in messages" :key="i" class="row" :class="m.role">
        <div v-if="m.role === 'user'" class="bubble user">{{ m.text }}</div>
        <div v-else-if="m.role === 'tool'" class="tool-chip">
          <Ico name="tool" :size="13" /> {{ m.text }}
        </div>
        <div v-else-if="m.role === 'error'" class="bubble err">{{ m.text }}</div>
        <div v-else class="bubble asst">
          <div class="md" v-html="renderMd(m.text)"></div>
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

/* ── 顶栏:悬浮琉璃三件套 ── */
.bar {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: calc(8px + var(--safe-top)) 12px 6px;
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

/* ── 底坞:建议条 + 输入卡片 ── */
.dock {
  padding: 0 10px calc(4px + var(--safe-bottom));
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
.foot {
  text-align: center;
  font-size: 11px;
  color: var(--text-faint);
  opacity: 0.7;
  padding: 7px 0 2px;
}
</style>

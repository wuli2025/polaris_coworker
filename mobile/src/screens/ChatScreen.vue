<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { messages, sending, sendMessage, cancel, newConversation } from "../lib/chat";
import { renderMd } from "../lib/md";
import { upload } from "../lib/net";
import { toast, toastErr } from "../lib/toast";
import { openPreview } from "../lib/preview";
import { hostName } from "../lib/auth";
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

// 空状态功能卡(豆包式):点一下把模板填进输入框,补上细节就能发
interface Feature {
  icon: string;
  label: string;
  hint: string;
  template: string;
}
const FEATURES: Feature[] = [
  {
    icon: "📊",
    label: "做 PPT",
    hint: "一句话生成整份演示",
    template: "帮我做一份 PPT,主题是:",
  },
  {
    icon: "🌐",
    label: "做网页",
    hint: "HTML 页面直接产出",
    template: "帮我做一个 HTML 网页,要求:",
  },
  {
    icon: "✍️",
    label: "写文档",
    hint: "报告/方案/文案",
    template: "帮我写一份文档,主题是:",
  },
  {
    icon: "📈",
    label: "数据分析",
    hint: "表格丢过来就行",
    template: "帮我分析这份数据,重点看:",
  },
  {
    icon: "🗂️",
    label: "总结文件",
    hint: "长文秒变要点",
    template: "帮我总结这份文件的要点",
  },
  {
    icon: "📅",
    label: "生成周报",
    hint: "口述内容变周报",
    template: "帮我生成一份周报,本周做了:",
  },
];
function useFeature(f: Feature) {
  draft.value = f.template;
  nextTick(() => {
    ta.value?.focus();
    // 光标停在模板末尾,接着补细节
    ta.value?.setSelectionRange(draft.value.length, draft.value.length);
  });
}

const modelLabel = computed(() => chosenProviderName() || "模型");
const skillLabel = computed(() =>
  chosenSkillIds.value.length ? `技能·${chosenSkillIds.value.length}` : "技能"
);
</script>

<template>
  <div class="chat">
    <header class="bar">
      <button class="icon" title="展开" @click="drawerOpen = true">☰</button>
      <div class="title">
        {{ hostName || "对话" }}
        <span class="dot" :class="{ live: wsOk }" :title="wsOk ? '已连接' : '连接中'"></span>
      </div>
      <button class="icon" title="新对话" @click="newConversation">＋</button>
    </header>

    <div ref="scroller" class="stream">
      <div v-if="!messages.length" class="empty">
        <div class="hi">嗨,今天做点什么?</div>
        <p class="muted sub">把任务丢给中控平台 —— PPT、网页、文档、数据都能直接产出。</p>
        <div class="feats">
          <button v-for="f in FEATURES" :key="f.label" class="feat" @click="useFeature(f)">
            <span class="fic">{{ f.icon }}</span>
            <span class="fmeta">
              <span class="flabel">{{ f.label }}</span>
              <span class="fhint">{{ f.hint }}</span>
            </span>
          </button>
        </div>
      </div>

      <div v-for="(m, i) in messages" :key="i" class="row" :class="m.role">
        <div v-if="m.role === 'user'" class="bubble user">{{ m.text }}</div>
        <div v-else-if="m.role === 'tool'" class="tool-chip">🔧 {{ m.text }}</div>
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
              <span class="ap-ico">👁</span>
              <span class="ap-name">{{ baseName(artView(m.artifacts).preview!) }}</span>
              <span class="ap-tag">预览</span>
            </button>
            <!-- 其余产物:默认折叠成一行「共 N 个」, 点开才列出, 不铺满小屏 -->
            <template v-if="artView(m.artifacts).rest.length">
              <button class="art-more" @click="toggleArts(i)">
                {{ artsExpanded.has(i) ? "收起" : `本轮共 ${m.artifacts.length} 个文件` }}
                <span class="am-chev">{{ artsExpanded.has(i) ? "▲" : "▼" }}</span>
              </button>
              <template v-if="artsExpanded.has(i)">
                <button
                  v-for="p in artView(m.artifacts).rest"
                  :key="p"
                  class="art"
                  @click="openPreview(p)"
                >
                  📎 {{ baseName(p) }}
                </button>
              </template>
            </template>
          </div>
        </div>
      </div>
      <div v-if="sending" class="typing">正在思考…</div>
    </div>

    <!-- 豆包式输入卡片:上排输入,下排 ＋/技能/模型/深度工作 chips + 发送 -->
    <div class="composer">
      <div class="card">
        <div v-if="attachments.length" class="atts">
          <span v-for="(a, i) in attachments" :key="i" class="att-chip">
            📎 {{ a.split(/[\\/]/).pop() }}
            <button @click="attachments.splice(i, 1)">✕</button>
          </span>
        </div>
        <textarea
          ref="ta"
          v-model="draft"
          class="ta"
          rows="1"
          placeholder="有什么想聊的?"
          @input="autoGrow"
          @keydown.enter.exact.prevent="send"
        ></textarea>
        <div class="tools">
          <button class="round" title="更多" @click="sheetOpen = true">＋</button>
          <div class="chips">
            <button
              v-if="skillState !== 'unsupported'"
              class="chip"
              :class="{ on: chosenSkillIds.length }"
              @click="skillOpen = true"
            >
              ⚡ {{ skillLabel }}
            </button>
            <button
              v-if="providerState !== 'unsupported'"
              class="chip"
              :class="{ on: chosenProviderId !== 'auto' }"
              @click="modelOpen = true"
            >
              ✦ {{ modelLabel }}
            </button>
            <button class="chip" :class="{ on: deepWork }" @click="deepWork = !deepWork">
              🧠 深度工作
            </button>
          </div>
          <button v-if="sending" class="stop" @click="cancel">■</button>
          <button v-else class="go" :disabled="!draft.trim()" @click="send">↑</button>
        </div>
      </div>
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
.bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: calc(10px + var(--safe-top)) 16px 10px;
  border-bottom: 1px solid var(--line);
  background: var(--bg-elev);
  -webkit-backdrop-filter: var(--blur);
  backdrop-filter: var(--blur);
}
.title {
  font-weight: 600;
  display: flex;
  align-items: center;
  gap: 8px;
}
.dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--text-faint);
}
.dot.live {
  background: var(--ok);
}
.icon {
  font-size: 22px;
  color: var(--text-dim);
  width: 34px;
  height: 34px;
}
.stream {
  flex: 1;
  overflow-y: auto;
  padding: 16px 14px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.empty {
  margin: auto 0;
  padding: 20px 4px;
}
.hi {
  font-size: 24px;
  font-weight: 700;
  margin-bottom: 6px;
}
.sub {
  margin: 0 0 18px;
  font-size: 14px;
}
.feats {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px;
}
.feat {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 13px 12px;
  background: var(--bg-elev);
  border: 1px solid var(--line);
  border-top-color: var(--line-hi);
  border-radius: 16px;
  text-align: left;
  transition: transform 0.1s;
}
.feat:active {
  transform: scale(0.96);
}
.fic {
  font-size: 24px;
}
.fmeta {
  display: flex;
  flex-direction: column;
  min-width: 0;
}
.flabel {
  font-size: 14.5px;
  font-weight: 600;
  color: var(--text);
}
.fhint {
  font-size: 11.5px;
  color: var(--text-faint);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.row {
  display: flex;
}
.row.user {
  justify-content: flex-end;
}
.bubble {
  max-width: 84%;
  padding: 10px 14px;
  border-radius: 16px;
  word-break: break-word;
}
.bubble.user {
  background: var(--user-bubble);
  border-bottom-right-radius: 5px;
}
.bubble.asst {
  background: var(--bg-elev);
  border: 1px solid var(--line);
  border-bottom-left-radius: 5px;
}
.bubble.err {
  background: transparent;
  border: 1px solid var(--danger);
  color: var(--danger);
}
.tool-chip {
  font-size: 13px;
  color: var(--text-dim);
  background: var(--bg-elev2);
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
  border: none;
  border-radius: 10px;
  background: var(--accent-soft, var(--bg-elev2));
  color: var(--text);
  text-align: left;
}
.art-preview .ap-ico {
  font-size: 16px;
  flex-shrink: 0;
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
  border-radius: 6px;
  background: var(--accent, #3b82f6);
  color: #fff;
}
/* 「共 N 个」折叠条 */
.art-more {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: 12.5px;
  padding: 4px 10px;
  border: none;
  border-radius: 8px;
  background: var(--bg-elev2);
  color: var(--text-dim);
}
.art-more .am-chev {
  font-size: 9px;
}
.art {
  font-size: 13px;
  padding: 4px 10px;
  background: var(--bg-elev2);
  border-radius: 8px;
  text-decoration: none;
}
.typing {
  color: var(--text-dim);
  font-size: 13px;
  padding-left: 4px;
}
/* ── 豆包式输入卡片 ── */
.composer {
  padding: 8px 10px calc(10px + var(--safe-bottom));
}
.card {
  background: var(--bg-elev);
  -webkit-backdrop-filter: var(--blur);
  backdrop-filter: var(--blur);
  border: 1px solid var(--line);
  border-top-color: var(--line-hi);
  border-radius: 22px;
  box-shadow: var(--shadow);
  padding: 10px 12px 8px;
}
.atts {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-bottom: 6px;
}
.att-chip {
  font-size: 12px;
  background: var(--bg-elev2);
  border-radius: 8px;
  padding: 4px 8px;
}
.att-chip button {
  color: var(--text-faint);
  margin-left: 4px;
}
.ta {
  display: block;
  width: 100%;
  resize: none;
  max-height: 140px;
  padding: 4px 4px 8px;
  background: transparent;
  border: none;
  outline: none;
  line-height: 1.45;
  font-size: 15.5px;
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
  background: var(--bg-elev2);
  border: 1px solid var(--line);
  font-size: 20px;
  line-height: 1;
  color: var(--text-dim);
  flex-shrink: 0;
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
  font-size: 12.5px;
  padding: 6px 11px;
  border-radius: 999px;
  background: var(--bg-elev2);
  border: 1px solid var(--line);
  color: var(--text-dim);
  transition: background 0.15s, color 0.15s, border-color 0.15s;
  white-space: nowrap;
}
.chip.on {
  background: var(--accent-soft);
  border-color: var(--accent);
  color: var(--accent);
}
.go,
.stop {
  width: 34px;
  height: 34px;
  border-radius: 50%;
  background: linear-gradient(135deg, var(--accent), var(--accent-2));
  color: #fff;
  font-size: 17px;
  flex-shrink: 0;
}
.go:disabled {
  background: var(--bg-elev2);
  color: var(--text-faint);
}
.stop {
  background: var(--danger);
  font-size: 13px;
}
</style>

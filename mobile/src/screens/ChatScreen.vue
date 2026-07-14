<script setup lang="ts">
import { nextTick, ref, watch } from "vue";
import { messages, sending, sendMessage, cancel, newConversation } from "../lib/chat";
import { renderMd } from "../lib/md";
import { upload } from "../lib/net";
import { toast, toastErr } from "../lib/toast";
import { openPreview } from "../lib/preview";
import { hostName } from "../lib/auth";
import TriggerSheet from "../components/TriggerSheet.vue";
import ChatDrawer from "../components/ChatDrawer.vue";

const props = defineProps<{ wsOk: boolean }>();

const draft = ref("");
const sheetOpen = ref(false);
const drawerOpen = ref(false);
const scroller = ref<HTMLElement | null>(null);
const fileInput = ref<HTMLInputElement | null>(null);
const attachments = ref<string[]>([]);

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
  const atts = attachments.value.slice();
  attachments.value = [];
  await sendMessage(t, atts.length ? { attachments: atts } : undefined);
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

const quicks = [
  "帮我梳理今天要做的事",
  "总结这份文件的要点",
  "生成一份周报",
];
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
        <div class="hi">要做点什么？</div>
        <p class="muted">下发任务，中控平台会替你执行 —— 文件、代码、报告都能做。</p>
        <div class="quicks">
          <button v-for="q in quicks" :key="q" class="quick" @click="draft = q">
            {{ q }}
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
            <button v-for="p in m.artifacts" :key="p" class="art" @click="openPreview(p)">
              📎 {{ p.split(/[\\/]/).pop() }}
            </button>
          </div>
        </div>
      </div>
      <div v-if="sending" class="typing">正在思考…</div>
    </div>

    <div class="composer">
      <div v-if="attachments.length" class="atts">
        <span v-for="(a, i) in attachments" :key="i" class="att-chip">
          📎 {{ a.split(/[\\/]/).pop() }}
          <button @click="attachments.splice(i, 1)">✕</button>
        </span>
      </div>
      <div class="input-row">
        <button class="plus" title="更多" @click="sheetOpen = true">＋</button>
        <textarea
          v-model="draft"
          class="ta"
          rows="1"
          placeholder="发消息给中控平台…"
          @input="autoGrow"
          @keydown.enter.exact.prevent="send"
        ></textarea>
        <button v-if="sending" class="stop" @click="cancel">■</button>
        <button v-else class="go" :disabled="!draft.trim()" @click="send">↑</button>
      </div>
    </div>

    <input ref="fileInput" type="file" multiple hidden @change="onFiles" />
    <TriggerSheet v-model="sheetOpen" @attach="pickFile" />
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
  text-align: center;
  padding: 30px 12px;
}
.hi {
  font-size: 22px;
  font-weight: 600;
  margin-bottom: 8px;
}
.quicks {
  margin-top: 20px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.quick {
  padding: 12px;
  background: var(--bg-elev);
  border: 1px solid var(--line);
  border-radius: 12px;
  color: var(--text);
  font-size: 14px;
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
.composer {
  border-top: 1px solid var(--line);
  padding: 10px 12px calc(10px + var(--safe-bottom));
  background: var(--bg-elev);
  -webkit-backdrop-filter: var(--blur);
  backdrop-filter: var(--blur);
}
.atts {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-bottom: 8px;
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
.input-row {
  display: flex;
  align-items: flex-end;
  gap: 8px;
}
.plus {
  width: 38px;
  height: 38px;
  border-radius: 50%;
  background: var(--bg-elev2);
  font-size: 22px;
  color: var(--text-dim);
  flex-shrink: 0;
}
.ta {
  flex: 1;
  resize: none;
  max-height: 140px;
  padding: 9px 14px;
  border-radius: 20px;
  background: var(--bg-elev2);
  border: 1px solid var(--line);
  outline: none;
  line-height: 1.4;
}
.ta:focus {
  border-color: var(--accent);
}
.go,
.stop {
  width: 38px;
  height: 38px;
  border-radius: 50%;
  background: var(--accent);
  color: #fff;
  font-size: 18px;
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

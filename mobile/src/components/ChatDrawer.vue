<script setup lang="ts">
/**
 * 左侧抽屉 —— 对话页左上角 ☰ 展开:
 *  顶部:当前主机(名称+连接状态) + 「切换主机」
 *  中部:**主机上的全部对话**(登录即有,手机/桌面发起的都在)+ 本地离线缓存兜底,
 *        合并去重、新→旧。点开续聊;仅本地的条目可 ✕ 清缓存。
 *  底部:新对话
 */
import { computed, watch } from "vue";
import {
  convList,
  convId,
  remoteConvs,
  remoteLoading,
  refreshRemoteConvs,
  openHostConversation,
  openConversation,
  removeConversation,
  newConversation,
} from "../lib/chat";
import { relTime } from "../lib/convs";
import { hostName, base } from "../lib/auth";
import { hostLabel } from "../lib/hosts";
import { go } from "../lib/nav";

const props = defineProps<{ modelValue: boolean; wsOk: boolean }>();
const emit = defineEmits<{ "update:modelValue": [boolean] }>();

// 打开抽屉即刷新主机对话列表(静默,已有内容时先显示旧的)
watch(
  () => props.modelValue,
  (open) => {
    if (open) refreshRemoteConvs();
  }
);

/** 合并:主机列表为真相,本地缓存独有的(离线期/主机已清)排后补充。 */
const merged = computed(() => {
  const seen = new Set(remoteConvs.value.map((c) => c.id));
  const localOnly = convList.value
    .filter((c) => !seen.has(c.id))
    .map((c) => ({ id: c.id, title: c.title, at: c.at, project: "", localOnly: true }));
  return [
    ...remoteConvs.value.map((c) => ({ ...c, localOnly: false })),
    ...localOnly,
  ].sort((a, b) => b.at - a.at);
});

function close() {
  emit("update:modelValue", false);
}
function pick(c: { id: string; localOnly: boolean }) {
  // 仅本机缓存的对话:主机上不存在,走本地恢复;若也请求主机,空 [] 会把缓存清屏(codex #9)。
  if (c.localOnly) openConversation(c.id);
  else openHostConversation(c.id);
  close();
}
function fresh() {
  newConversation();
  close();
}
function switchHost() {
  close();
  go("hosts");
}
function del(id: string, e: Event) {
  e.stopPropagation();
  removeConversation(id);
}
</script>

<template>
  <transition name="fade">
    <div v-if="props.modelValue" class="scrim" @click="close"></div>
  </transition>
  <transition name="drawer">
    <aside v-if="props.modelValue" class="drawer glass-panel">
      <div class="dhead">
        <div class="hrow">
          <span class="hdot" :class="{ live: wsOk }"></span>
          <div class="hmeta">
            <div class="hname">{{ hostName || "未命名主机" }}</div>
            <div class="haddr faint">{{ hostLabel(base) }}</div>
          </div>
        </div>
        <button class="btn ghost sw" @click="switchHost">⇄ 切换主机</button>
      </div>

      <div class="dlabel faint">
        对话记录{{ remoteLoading ? " · 同步中…" : "" }}
      </div>
      <div class="dlist">
        <p v-if="!merged.length" class="dempty faint">
          {{ remoteLoading ? "正在从主机拉取…" : "还没有对话,发一句就有了" }}
        </p>
        <div
          v-for="c in merged"
          :key="c.id"
          class="conv"
          :class="{ on: c.id === convId }"
          @click="pick(c)"
        >
          <div class="cmeta">
            <div class="ctitle">{{ c.title }}</div>
            <div class="ctime faint">
              {{ relTime(c.at) }}<template v-if="c.project"> · {{ c.project }}</template>
              <template v-if="c.localOnly"> · 仅本机缓存</template>
            </div>
          </div>
          <button v-if="c.localOnly" class="cdel" title="清掉本机缓存" @click="del(c.id, $event)">✕</button>
        </div>
      </div>

      <button class="btn full new" @click="fresh">＋ 新对话</button>
    </aside>
  </transition>
</template>

<style scoped>
.scrim {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.45);
  z-index: 200;
}
.drawer {
  position: fixed;
  top: 0;
  left: 0;
  bottom: 0;
  width: min(82vw, 320px);
  z-index: 210;
  display: flex;
  flex-direction: column;
  padding: calc(14px + var(--safe-top)) 14px calc(14px + var(--safe-bottom));
}
.dhead {
  padding-bottom: 14px;
  border-bottom: 1px solid var(--line);
}
.hrow {
  display: flex;
  align-items: center;
  gap: 10px;
}
.hdot {
  width: 9px;
  height: 9px;
  border-radius: 50%;
  background: var(--text-faint);
  flex-shrink: 0;
}
.hdot.live {
  background: var(--ok);
  box-shadow: 0 0 8px var(--ok);
}
.hmeta {
  min-width: 0;
}
.hname {
  font-weight: 600;
  font-size: 16px;
}
.haddr {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.sw {
  width: 100%;
  margin-top: 12px;
  padding: 9px;
  font-size: 14px;
}
.dlabel {
  margin: 14px 2px 8px;
  font-size: 12px;
}
.dlist {
  flex: 1;
  overflow-y: auto;
  min-height: 0;
}
.dempty {
  text-align: center;
  padding: 30px 0;
}
.conv {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 11px 10px;
  border-radius: 12px;
  cursor: pointer;
}
.conv.on {
  background: var(--accent-soft);
}
.conv:active {
  background: var(--bg-elev2);
}
.cmeta {
  flex: 1;
  min-width: 0;
}
.ctitle {
  font-size: 14px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.ctime {
  font-size: 11px;
}
.cdel {
  color: var(--text-faint);
  font-size: 12px;
  padding: 6px;
  flex-shrink: 0;
}
.new {
  margin-top: 12px;
}
.drawer-enter-active,
.drawer-leave-active {
  transition: transform 0.22s ease;
}
.drawer-enter-from,
.drawer-leave-to {
  transform: translateX(-100%);
}
</style>

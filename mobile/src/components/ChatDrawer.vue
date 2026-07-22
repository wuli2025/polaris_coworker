<script setup lang="ts">
/**
 * 左侧抽屉(豆包式)—— 对话页左上角 ☰ 展开:
 *  最顶:「＋ 新对话」(第一操作,放最上面)
 *  其下:当前主机(名称+连接状态+⇄切换)
 *  中部:**主机上的全部对话**(登录即有,手机/桌面发起的都在)+ 本地离线缓存兜底,
 *        合并去重、新→旧。点开续聊;仅本地的条目可 ✕ 清缓存。
 *  底部:文件/项目快捷入口(能力管控可隐藏) + 用户行(头像+名字 → 设置)
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
import { hostName, base, user, displayName } from "../lib/auth";
import { hostLabel } from "../lib/hosts";
import { hasCap } from "../lib/capabilities";
import { go, type Screen } from "../lib/nav";

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
function nav(s: Screen) {
  close();
  go(s);
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
      <!-- 最顶:新对话 -->
      <button class="btn full new" @click="fresh">＋ 新对话</button>

      <div class="dhead">
        <div class="hrow">
          <span class="hdot" :class="{ live: wsOk }"></span>
          <div class="hmeta">
            <div class="hname">{{ hostName || "未命名主机" }}</div>
            <div class="haddr faint">{{ hostLabel(base) }}</div>
          </div>
          <button class="hsw" title="切换主机" @click="nav('hosts')">⇄</button>
        </div>
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

      <!-- 底部:快捷入口 + 设置 -->
      <div class="dfoot">
        <div class="shortcuts">
          <button v-if="hasCap('files')" class="short" @click="nav('files')">
            <span class="sic">📁</span>文件
          </button>
          <button v-if="hasCap('projects')" class="short" @click="nav('projects')">
            <span class="sic">🗂️</span>项目
          </button>
          <button v-if="hasCap('kb')" class="short" @click="nav('kb')">
            <span class="sic">📚</span>知识库
          </button>
        </div>
        <button class="urow" @click="nav('settings')">
          <span class="avatar">{{ displayName(user).slice(0, 1) }}</span>
          <span class="uname">{{ displayName(user) }}</span>
          <span class="uset faint">⚙️ 设置</span>
        </button>
      </div>
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
  padding: calc(14px + var(--safe-top)) 14px calc(10px + var(--safe-bottom));
}
.new {
  flex-shrink: 0;
}
.dhead {
  padding: 12px 0;
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
  flex: 1;
}
.hname {
  font-weight: 600;
  font-size: 15px;
}
.haddr {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 12px;
}
.hsw {
  width: 34px;
  height: 34px;
  border-radius: 10px;
  background: var(--bg-elev2);
  border: 1px solid var(--line);
  color: var(--text-dim);
  font-size: 15px;
  flex-shrink: 0;
}
.dlabel {
  margin: 12px 2px 6px;
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
/* ── 底部:快捷入口 + 用户/设置 ── */
.dfoot {
  flex-shrink: 0;
  border-top: 1px solid var(--line);
  padding-top: 10px;
  margin-top: 8px;
}
.shortcuts {
  display: flex;
  gap: 8px;
  margin-bottom: 10px;
}
.short {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
  padding: 9px 0 7px;
  border-radius: 12px;
  background: var(--bg-elev2);
  border: 1px solid var(--line);
  color: var(--text-dim);
  font-size: 12px;
}
.short:active {
  background: var(--accent-soft);
}
.sic {
  font-size: 19px;
}
.urow {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 9px 8px;
  border-radius: 12px;
  text-align: left;
}
.urow:active {
  background: var(--bg-elev2);
}
.avatar {
  width: 34px;
  height: 34px;
  border-radius: 50%;
  background: var(--accent);
  color: #fff;
  display: flex;
  align-items: center;
  justify-content: center;
  font-weight: 600;
  font-size: 15px;
  flex-shrink: 0;
}
.uname {
  flex: 1;
  font-weight: 500;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.uset {
  flex-shrink: 0;
}
.full {
  width: 100%;
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

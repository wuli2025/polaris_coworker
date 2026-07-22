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
import Ico from "./Ico.vue";

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
      <!-- 顶:当前主机(状态灯 + 地址 + 切换) -->
      <div class="dhead">
        <div class="hrow">
          <span class="hdot" :class="{ live: wsOk }"></span>
          <div class="hmeta">
            <div class="hname">{{ hostName || "未命名主机" }}</div>
            <div class="haddr faint">{{ hostLabel(base) }}</div>
          </div>
          <button class="hsw" title="切换主机" @click="nav('hosts')">
            <Ico name="swap" :size="17" />
          </button>
        </div>
      </div>

      <!-- 新对话:玻璃行,不用整块渐变按钮(太抢眼) -->
      <button class="new" @click="fresh">
        <span class="nico"><Ico name="compose" :size="16" /></span>
        新对话
      </button>

      <div class="dlabel">
        <span>对话记录</span>
        <span v-if="remoteLoading" class="syncing">同步中</span>
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
          <button v-if="c.localOnly" class="cdel" title="清掉本机缓存" @click="del(c.id, $event)">
            <Ico name="close" :size="13" />
          </button>
        </div>
      </div>

      <!-- 底部:快捷入口 + 设置 -->
      <div class="dfoot">
        <div class="shortcuts">
          <button v-if="hasCap('files')" class="short" @click="nav('files')">
            <Ico name="folder" :size="18" />文件
          </button>
          <button v-if="hasCap('projects')" class="short" @click="nav('projects')">
            <Ico name="grid" :size="18" />项目
          </button>
          <button v-if="hasCap('kb')" class="short" @click="nav('kb')">
            <Ico name="book" :size="18" />知识库
          </button>
        </div>
        <button class="urow" @click="nav('settings')">
          <span class="avatar">{{ displayName(user).slice(0, 1) }}</span>
          <span class="uname">{{ displayName(user) }}</span>
          <span class="uset"><Ico name="settings" :size="17" /></span>
        </button>
      </div>
    </aside>
  </transition>
</template>

<style scoped>
.scrim {
  position: fixed;
  inset: 0;
  background: var(--scrim);
  /* 只虚化不压暗:scrim 一旦 brightness 变暗,上面的抽屉会去采样这层暗画面 → 整片发灰
     (实测过,别加回来)。景深靠虚化 + 半透明黑给,不靠降亮度。 */
  -webkit-backdrop-filter: blur(10px);
  backdrop-filter: blur(10px);
  z-index: 200;
}
.drawer {
  position: fixed;
  top: 0;
  left: 0;
  bottom: 0;
  width: min(84vw, 328px);
  z-index: 210;
  display: flex;
  flex-direction: column;
  padding: calc(12px + var(--safe-top)) 12px calc(10px + var(--safe-bottom));
  border-radius: 0 var(--radius-lg) var(--radius-lg) 0;
  overflow: hidden;
  box-shadow: var(--shadow-lg);
}
/* 右缘一道竖向镜面光:玻璃板的「厚度」,两端渐隐免得在圆角处露直角 */
.drawer::after {
  content: "";
  position: absolute;
  top: 0;
  right: 0;
  bottom: 0;
  width: 1.5px;
  background: linear-gradient(
    180deg,
    transparent,
    var(--edge) 16%,
    var(--edge) 74%,
    transparent
  );
  pointer-events: none;
  z-index: 1;
}
/* 抽屉内部所有行都不再单独 blur:面板已经是一片玻璃,里面再 blur 只会越叠越灰 */
.drawer > * {
  position: relative;
  z-index: 1;
}
.dhead {
  padding: 4px 4px 12px;
}
/* 新对话:玻璃行 */
.new {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  flex-shrink: 0;
  padding: 11px 12px;
  border-radius: 16px;
  background: var(--glass2);
  border: 1px solid var(--line);
  border-top-color: var(--line-hi);
  border-bottom-color: var(--line-lo);
  font-size: 14.5px;
  font-weight: 500;
  text-align: left;
  box-shadow: var(--shadow-sm);
  transition: transform 0.13s cubic-bezier(0.32, 0.72, 0, 1), filter 0.13s ease;
  touch-action: manipulation;
}
.new:active,
.hsw:active,
.short:active {
  transform: scale(0.96);
  filter: brightness(0.9);
}
.nico {
  width: 28px;
  height: 28px;
  border-radius: 9px;
  display: grid;
  place-items: center;
  background: linear-gradient(140deg, var(--accent), var(--accent-2));
  color: #fff;
  flex-shrink: 0;
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
  border-radius: 11px;
  background: var(--glass2);
  border: 1px solid var(--line);
  border-top-color: var(--line-hi);
  border-bottom-color: var(--line-lo);
  color: var(--text-dim);
  display: grid;
  place-items: center;
  flex-shrink: 0;
  transition: transform 0.13s cubic-bezier(0.32, 0.72, 0, 1), filter 0.13s ease;
  touch-action: manipulation;
}
.dlabel {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 16px 6px 4px;
  font-size: 11.5px;
  letter-spacing: 0.6px;
  color: var(--text-faint);
}
.syncing {
  font-size: 11px;
  padding: 1px 7px;
  border-radius: 999px;
  background: var(--glass2);
  color: var(--text-faint);
  animation: pulse 1.4s ease-in-out infinite;
}
@keyframes pulse {
  0%, 100% { opacity: 0.45 }
  50% { opacity: 1 }
}
.dlist {
  flex: 1;
  overflow-y: auto;
  min-height: 0;
  margin: 0 -2px;
  padding: 0 2px;
}
.dempty {
  text-align: center;
  padding: 30px 0;
}
.conv {
  position: relative;
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 10px 10px;
  border-radius: 13px;
  cursor: pointer;
  border: 1px solid transparent;
}
/* 选中态:淡玻璃 + 左侧一道竖条,不用整块高饱和色块 */
.conv.on {
  background: var(--glass2);
  border-color: var(--line);
  border-top-color: var(--line-hi);
}
.conv.on::before {
  content: "";
  position: absolute;
  left: 4px;
  top: 50%;
  transform: translateY(-50%);
  width: 3px;
  height: 16px;
  border-radius: 2px;
  background: linear-gradient(180deg, var(--accent), var(--accent-2));
}
.conv.on .ctitle {
  font-weight: 600;
}
.conv:active {
  background: var(--glass2);
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
  padding: 6px;
  flex-shrink: 0;
  display: flex;
}
/* ── 底部:快捷入口 + 用户/设置 ── */
.dfoot {
  flex-shrink: 0;
  padding-top: 12px;
  margin-top: 8px;
  position: relative;
}
/* 发丝分隔线:两端渐隐,比整条实线轻 */
.dfoot::before {
  content: "";
  position: absolute;
  top: 0;
  left: 6px;
  right: 6px;
  height: 1px;
  background: linear-gradient(90deg, transparent, var(--line-hi), transparent);
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
  gap: 6px;
  padding: 10px 0 8px;
  border-radius: 14px;
  background: var(--glass2);
  border: 1px solid var(--line);
  border-top-color: var(--line-hi);
  border-bottom-color: var(--line-lo);
  color: var(--text-dim);
  font-size: 11.5px;
  box-shadow: var(--shadow-sm);
  transition: transform 0.13s cubic-bezier(0.32, 0.72, 0, 1), filter 0.13s ease;
  touch-action: manipulation;
}
.urow {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px;
  border-radius: 14px;
  text-align: left;
}
.urow:active {
  background: var(--glass2);
}
.avatar {
  width: 34px;
  height: 34px;
  border-radius: 50%;
  background: linear-gradient(140deg, var(--accent), var(--accent-2));
  box-shadow: inset 0 -5px 10px rgba(30, 20, 90, 0.3), 0 3px 10px rgba(108, 140, 255, 0.3);
  color: #fff;
  display: flex;
  align-items: center;
  justify-content: center;
  font-weight: 600;
  font-size: 15px;
  flex-shrink: 0;
  text-transform: uppercase;
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
  color: var(--text-faint);
  display: flex;
  padding: 6px;
}
.drawer-enter-active,
.drawer-leave-active {
  transition: transform 0.26s cubic-bezier(0.32, 0.72, 0, 1), opacity 0.2s;
}
.drawer-enter-from,
.drawer-leave-to {
  transform: translateX(-100%);
  opacity: 0.7;
}
</style>

<script setup lang="ts">
/**
 * 全局任务中心 —— 常驻右下角的后台任务浮层。
 *
 * 解决「点了盘点/建索引/智能归类/构建知识网,一切走那个页面就好像停了、也看不到进度」:
 * 这些任务的真身都是后端后台线程 + 全局事件(见 stores/fileTasks.ts、stores/kb.ts),
 * 本组件常驻挂在 App.vue,从两个全局 store 读运行态 → 无论当前在哪个视图,只要有任务在跑
 * 就在这里显示「还在跑 + 实时进度」,点一下即可跳回对应页面查看。空闲时自动隐藏,零干扰。
 */
import { computed, ref } from "vue";
import { LoaderCircle, ChevronDown, Activity, FileText, X } from "@lucide/vue";
import { useFileTasksStore } from "../stores/fileTasks";
import { useKbStore } from "../stores/kb";
import { useAppStore, type ViewKey } from "../stores/app";
import { artifacts as artifactsApi } from "../tauri";

const tasks = useFileTasksStore();
const kb = useKbStore();
const app = useAppStore();

const expanded = ref(true);

interface Row {
  key: string;
  label: string;
  detail: string;
  view: ViewKey;
  report?: string;
}

// 汇总两个 store 里所有「正在跑」的任务成统一列表。
const rows = computed<Row[]>(() => {
  const out: Row[] = [];
  for (const t of tasks.activeList) {
    out.push({
      key: "ft:" + t.id,
      label: t.label,
      detail: t.detail,
      view: "file_center",
      report: t.id === "clusterLlm" ? tasks.reportPath.clusterLlm : undefined,
    });
  }
  if (kb.compiling) {
    const last = kb.compileLog.length ? kb.compileLog[kb.compileLog.length - 1] : "";
    out.push({
      key: "kb:compile",
      label: "构建知识网",
      detail: (kb.compileMsg || last || "进行中…").replace(/^[▸·📄⚠]\s*/, ""),
      view: "wiki",
    });
  }
  return out;
});

const show = computed(() => rows.value.length > 0);
const count = computed(() => rows.value.length);

function goto(r: Row) {
  app.setView(r.view);
}
function openReport(path?: string) {
  if (path) artifactsApi.openExternal(path).catch(() => {});
}
</script>

<template>
  <transition name="tc">
    <div v-if="show" class="task-center" :class="{ collapsed: !expanded }">
      <button class="tc-head" @click="expanded = !expanded">
        <span class="tc-pulse"><Activity :size="14" :stroke-width="2" /></span>
        <span class="tc-title">后台任务</span>
        <span class="tc-count">{{ count }}</span>
        <ChevronDown class="tc-chev" :class="{ flip: !expanded }" :size="15" :stroke-width="2" />
      </button>
      <div v-show="expanded" class="tc-body">
        <div v-for="r in rows" :key="r.key" class="tc-row" @click="goto(r)" :title="'点击跳转查看 · ' + r.label">
          <LoaderCircle :size="15" class="tc-spin" />
          <div class="tc-main">
            <div class="tc-label">{{ r.label }}</div>
            <div class="tc-detail">{{ r.detail }}</div>
          </div>
          <button
            v-if="r.report"
            class="tc-report"
            title="打开桌面报告"
            @click.stop="openReport(r.report)"
          >
            <FileText :size="13" :stroke-width="1.9" />
          </button>
        </div>
      </div>
      <!-- 收起态:只显示一行最紧凑的胶囊(数量 + 第一个任务名) -->
      <div v-show="!expanded" class="tc-mini" @click="expanded = true">
        <LoaderCircle :size="13" class="tc-spin" />
        <span>{{ rows[0]?.label }}{{ count > 1 ? ` 等 ${count} 项` : "" }}</span>
      </div>
    </div>
  </transition>
</template>

<style scoped>
.task-center {
  position: fixed;
  right: 18px;
  bottom: 18px;
  z-index: 9990;
  width: 290px;
  background: var(--bg-side, #1f1f1f);
  border: 1px solid var(--hairline, rgba(255, 255, 255, 0.08));
  border-radius: 14px;
  box-shadow: var(--shadow-lg, 0 12px 40px rgba(0, 0, 0, 0.4));
  overflow: hidden;
  backdrop-filter: blur(18px) saturate(1.2);
  -webkit-backdrop-filter: blur(18px) saturate(1.2);
}
.task-center.collapsed {
  width: auto;
}
.tc-head {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 10px 12px;
  background: transparent;
  border: none;
  cursor: pointer;
  color: var(--muted, #a8a8a4);
  font-size: 12.5px;
  letter-spacing: 0.04em;
}
.collapsed .tc-head {
  display: none;
}
.tc-pulse {
  display: inline-flex;
  color: var(--primary, #d4b06a);
  animation: tc-breathe 2s ease-in-out infinite;
}
@keyframes tc-breathe {
  0%, 100% { opacity: 0.55; }
  50% { opacity: 1; }
}
.tc-title {
  font-weight: 650;
  color: var(--ink, #e8e8e6);
}
.tc-count {
  min-width: 18px;
  height: 18px;
  padding: 0 5px;
  border-radius: 9px;
  background: var(--primary, #d4b06a);
  color: #1a1a1a;
  font-size: 11px;
  font-weight: 700;
  display: inline-flex;
  align-items: center;
  justify-content: center;
}
.tc-chev {
  margin-left: auto;
  color: var(--muted, #888);
  transition: transform 0.2s ease;
}
.tc-chev.flip {
  transform: rotate(180deg);
}
.tc-body {
  padding: 2px 8px 8px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.tc-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 10px;
  border-radius: 10px;
  cursor: pointer;
  transition: background 0.15s ease;
}
.tc-row:hover {
  background: var(--primary-soft, rgba(212, 176, 106, 0.12));
}
.tc-main {
  min-width: 0;
  flex: 1;
}
.tc-label {
  font-size: 12.5px;
  font-weight: 600;
  color: var(--ink, #e8e8e6);
}
.tc-detail {
  font-size: 11px;
  color: var(--muted, #999);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  margin-top: 1px;
}
.tc-spin {
  color: var(--primary, #d4b06a);
  animation: tc-rot 0.9s linear infinite;
  flex-shrink: 0;
}
@keyframes tc-rot {
  to { transform: rotate(360deg); }
}
.tc-report {
  flex-shrink: 0;
  background: transparent;
  border: 1px solid var(--hairline, rgba(255, 255, 255, 0.1));
  border-radius: 8px;
  padding: 5px;
  color: var(--muted, #999);
  cursor: pointer;
  display: inline-flex;
}
.tc-report:hover {
  color: var(--primary, #d4b06a);
  border-color: var(--primary, #d4b06a);
}
.tc-mini {
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 9px 14px;
  cursor: pointer;
  color: var(--ink, #e8e8e6);
  font-size: 12px;
  font-weight: 600;
  white-space: nowrap;
}
.tc-enter-active,
.tc-leave-active {
  transition: opacity 0.25s ease, transform 0.25s ease;
}
.tc-enter-from,
.tc-leave-to {
  opacity: 0;
  transform: translateY(10px);
}
</style>

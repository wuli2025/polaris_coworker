<script setup lang="ts">
import { computed } from "vue";
import { go } from "../lib/nav";
import { CAP_CATALOG, type CapKey } from "../lib/capabilities";

const props = defineProps<{ which: CapKey }>();
const meta = computed(() => CAP_CATALOG.find((c) => c.key === props.which));

// 这些能力后端命令已就绪(技能/知识库/语音/管理),移动端 UI 迭代中;
// 先给统一入口占位,避免"更多"面板点了没反应。核心链路(对话/文件/项目)已可用。
const HINT: Record<string, string> = {
  skills: "在对话里直接说出要用的技能，中控平台会自动触发对应工作流。",
  kb: "在对话里提问，平台会自动检索团队知识库作答；独立检索界面即将上线。",
  voice: "语音输入即将上线；当前可用系统输入法的语音转文字。",
  admin: "成员 / 设备 / 权限管理请暂用桌面端或 Web 端；移动端管理面板开发中。",
};
</script>

<template>
  <div class="ph">
    <header class="bar">
      <button class="icon" @click="go('chat')">‹</button>
      <div class="title">{{ meta?.label }}</div>
      <span style="width: 34px"></span>
    </header>
    <div class="body">
      <div class="big">{{ meta?.icon }}</div>
      <h2>{{ meta?.label }}</h2>
      <p class="muted">{{ HINT[which] }}</p>
      <button class="btn" @click="go('chat')">回到对话</button>
    </div>
  </div>
</template>

<style scoped>
.ph {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-height: 0;
}
.bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: calc(10px + var(--safe-top)) 12px 10px;
  border-bottom: 1px solid var(--line);
}
.title {
  font-weight: 600;
}
.icon {
  font-size: 26px;
  color: var(--text-dim);
  width: 34px;
}
.body {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  text-align: center;
  padding: 30px;
  gap: 10px;
}
.big {
  font-size: 56px;
}
h2 {
  margin: 4px 0;
}
.body p {
  max-width: 280px;
  margin-bottom: 12px;
}
</style>

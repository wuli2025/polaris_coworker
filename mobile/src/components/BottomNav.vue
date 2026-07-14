<script setup lang="ts">
import { computed } from "vue";
import { screen, go, type Screen } from "../lib/nav";
import { hasCap } from "../lib/capabilities";

interface Tab {
  key: Screen;
  label: string;
  icon: string;
  cap?: Parameters<typeof hasCap>[0];
}
const ALL: Tab[] = [
  { key: "chat", label: "对话", icon: "💬", cap: "chat" },
  { key: "files", label: "文件", icon: "📁", cap: "files" },
  { key: "projects", label: "项目", icon: "🗂️", cap: "projects" },
  { key: "settings", label: "我的", icon: "⚙️" },
];
const tabs = computed(() => ALL.filter((t) => !t.cap || hasCap(t.cap)));
</script>

<template>
  <nav class="nav">
    <button
      v-for="t in tabs"
      :key="t.key"
      class="tab"
      :class="{ on: screen === t.key }"
      @click="go(t.key)"
    >
      <span class="ic">{{ t.icon }}</span>
      <span class="lb">{{ t.label }}</span>
    </button>
  </nav>
</template>

<style scoped>
/* iOS 悬浮玻璃 Dock:离边、圆角药丸、毛玻璃,内容从下面滚过 */
.nav {
  display: flex;
  margin: 6px 12px calc(10px + var(--safe-bottom));
  padding: 4px;
  border-radius: 999px;
  background: var(--bg-elev);
  -webkit-backdrop-filter: var(--blur);
  backdrop-filter: var(--blur);
  border: 1px solid var(--line);
  border-top-color: var(--line-hi);
  box-shadow: var(--shadow);
}
.tab {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 3px;
  padding: 7px 0 5px;
  border-radius: 999px;
  color: var(--text-faint);
  transition: background 0.18s, color 0.18s;
}
.tab.on {
  color: var(--accent);
  background: var(--accent-soft);
}
.ic {
  font-size: 20px;
}
.lb {
  font-size: 11px;
}
</style>

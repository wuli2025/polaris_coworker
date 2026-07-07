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
.nav {
  display: flex;
  border-top: 1px solid var(--line);
  background: var(--bg-elev);
  padding-bottom: var(--safe-bottom);
}
.tab {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 3px;
  padding: 8px 0 6px;
  color: var(--text-faint);
}
.tab.on {
  color: var(--accent);
}
.ic {
  font-size: 20px;
}
.lb {
  font-size: 11px;
}
</style>

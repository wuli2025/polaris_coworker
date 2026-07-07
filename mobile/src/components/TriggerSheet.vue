<script setup lang="ts">
import { computed } from "vue";
import { CAP_CATALOG, hasCap, type CapKey } from "../lib/capabilities";
import { go } from "../lib/nav";

const props = defineProps<{ modelValue: boolean }>();
const emit = defineEmits<{
  "update:modelValue": [boolean];
  attach: [];
}>();

// 触发面板里展示的能力(排除聊天本身,按角色/覆盖过滤)
const tiles = computed(() =>
  CAP_CATALOG.filter((c) => c.key !== "chat" && hasCap(c.key))
);

function close() {
  emit("update:modelValue", false);
}
function open(key: CapKey) {
  close();
  go(key);
}
function attach() {
  close();
  emit("attach");
}
</script>

<template>
  <transition name="fade">
    <div v-if="props.modelValue" class="scrim" @click="close"></div>
  </transition>
  <transition name="sheet">
    <div v-if="props.modelValue" class="sheet">
      <div class="grabber"></div>
      <h3>更多能力</h3>
      <div class="grid">
        <button class="tile" @click="attach">
          <span class="ic">📎</span><span>附件</span>
        </button>
        <button v-for="t in tiles" :key="t.key" class="tile" @click="open(t.key)">
          <span class="ic">{{ t.icon }}</span><span>{{ t.label }}</span>
        </button>
      </div>
      <p class="faint hint">能力可在「设置 · 能力管控」里隐藏或开启。</p>
    </div>
  </transition>
</template>

<style scoped>
.scrim {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.45);
  z-index: 150;
}
.sheet {
  position: fixed;
  left: 0;
  right: 0;
  bottom: 0;
  z-index: 160;
  background: var(--bg-elev);
  border-top-left-radius: 22px;
  border-top-right-radius: 22px;
  padding: 8px 18px calc(20px + var(--safe-bottom));
  border-top: 1px solid var(--line);
}
.grabber {
  width: 40px;
  height: 4px;
  border-radius: 2px;
  background: var(--line);
  margin: 6px auto 12px;
}
h3 {
  margin: 0 0 14px;
  font-size: 16px;
}
.grid {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 12px;
}
.tile {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 7px;
  padding: 14px 4px;
  background: var(--bg-elev2);
  border-radius: 14px;
  color: var(--text);
  font-size: 12px;
}
.tile:active {
  transform: scale(0.95);
}
.ic {
  font-size: 24px;
}
.hint {
  margin-top: 14px;
  text-align: center;
}
</style>

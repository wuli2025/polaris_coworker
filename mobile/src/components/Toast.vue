<script setup lang="ts">
import { toasts } from "../lib/toast";
</script>

<template>
  <div class="toast-host">
    <transition-group name="fade">
      <div v-for="t in toasts" :key="t.id" class="toast" :class="t.kind">
        {{ t.text }}
      </div>
    </transition-group>
  </div>
</template>

<style scoped>
.toast-host {
  position: fixed;
  left: 0;
  right: 0;
  bottom: calc(84px + var(--safe-bottom));
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  pointer-events: none;
  z-index: 200;
}
.toast {
  max-width: 84%;
  padding: 10px 16px;
  border-radius: 12px;
  background: var(--bg-elev2);
  border: 1px solid var(--line);
  box-shadow: 0 6px 24px rgba(0, 0, 0, 0.3);
  font-size: 14px;
}
.toast.error {
  border-color: var(--danger);
  color: var(--danger);
}
.toast.ok {
  border-color: var(--ok);
}
</style>

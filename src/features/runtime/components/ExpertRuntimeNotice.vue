<script setup lang="ts">
import { useBackendStatus } from '../useBackendStatus';
import { runtimeCopy as copy } from '../copy';
import { watch } from 'vue';
import { useExpertSettings } from '../useExpertSettings';
const { legacy, isPi } = useBackendStatus();
const { enabled, busy, error, refresh, set } = useExpertSettings();
watch(isPi, value => { if (value) void refresh(); }, { immediate: true });
</script>
<template>
  <aside v-if="!legacy">
    <p>{{ copy.expertRequest }}</p>
    <label><input type="checkbox" :disabled="busy || enabled === null || !isPi" :checked="enabled === true"
      @change="set(($event.target as HTMLInputElement).checked)" />
      专家团：{{ enabled === null ? '状态未知' : enabled ? '已开启' : '已关闭' }}（默认关闭）</label>
    <button :disabled="busy || !isPi" @click="refresh">刷新状态</button>
    <p>设置会持久化到宿主；新会话立即生效，空闲会话自动重开，正在执行的会话于本轮结束后生效。</p>
    <p v-if="error" role="alert">{{ error }}</p>
  </aside>
</template>
<style scoped>
aside { padding: 12px 20px; font-size: 12px; line-height: 1.6; color: var(--muted); }
p { margin: 4px 0; }
</style>

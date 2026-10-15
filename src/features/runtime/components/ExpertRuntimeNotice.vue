<script setup lang="ts">
import { useBackendStatus } from '../useBackendStatus';
import { runtimeCopy as copy } from '../copy';
const { legacy, error } = useBackendStatus();
</script>
<template>
  <aside v-if="!legacy">
    <p>{{ copy.expertRequest }}</p>
    <label><input type="checkbox" disabled :checked="false" /> {{ copy.expertSwitch }}</label>
    <p>{{ copy.expertBlocked }}</p>
    <code>{{ copy.expertConfig }}</code>
    <p v-if="error">{{ error }}</p>
  </aside>
</template>
<style scoped>
aside { padding: 12px 20px; font-size: 12px; line-height: 1.6; color: var(--muted); }
p { margin: 4px 0; }
</style>

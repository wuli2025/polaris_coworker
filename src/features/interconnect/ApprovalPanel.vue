<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue';
import type { Approval } from './contracts';
import { interconnectCopy as c } from './copy';
defineProps<{ approvals: Approval[]; pending: string; error: string; busy: boolean }>();
defineEmits<{ decide: [id: string, approve: boolean]; retry: []; cancel: [] }>();
const now = ref(Date.now());
let timer: ReturnType<typeof setInterval>;
onMounted(() => { timer = setInterval(() => { now.value = Date.now(); }, 1000); });
onUnmounted(() => clearInterval(timer));
</script>
<template>
  <section aria-live="polite">
    <p v-if="pending">{{ c.waiting }} <button :disabled="busy" @click="$emit('cancel')">{{ c.cancel }}</button></p>
    <p v-if="error" role="alert">{{ error }} <button :disabled="busy" @click="$emit('retry')">{{ c.retry }}</button></p>
    <h2 v-if="approvals.length">{{ c.approvals }}</h2>
    <article v-for="item in approvals.filter(a => a.id !== pending && a.state === 'pending')" :key="item.id">
      <p>{{ c.operations[item.operation] || item.operation }} · {{ c.device }}：{{ item.actor }}</p>
      <p>{{ c.remaining }} {{ Math.max(0, Math.ceil(item.expiresAt - now / 1000)) }} {{ c.seconds }}</p>
      <button :disabled="busy || item.expiresAt * 1000 <= now" @click="$emit('decide', item.id, true)">{{ c.approve }}</button>
      <button :disabled="busy || item.expiresAt * 1000 <= now" @click="$emit('decide', item.id, false)">{{ c.deny }}</button>
    </article>
  </section>
</template>

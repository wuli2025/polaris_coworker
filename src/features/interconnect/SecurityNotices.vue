<script setup lang="ts">
import type { Notice } from './contracts';
import { interconnectCopy as c } from './copy';
defineProps<{ notices: Notice[]; busy: boolean }>();
defineEmits<{ ack: [id: string]; revoke: [notice: Notice] }>();
</script>
<template>
  <section v-if="notices.length" aria-live="polite">
    <h2>{{ c.security }}</h2>
    <article v-for="notice in notices" :key="notice.deviceId">
      <p>{{ notice.name }} · {{ notice.platform }} · {{ new Date(notice.time * 1000).toLocaleString() }}</p>
      <button :disabled="busy" @click="$emit('revoke', notice)">{{ c.revoke }}</button>
      <button @click="$emit('ack', notice.deviceId)">{{ c.ack }}</button>
    </article>
  </section>
</template>

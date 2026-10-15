<script setup lang="ts">
import { modelConfigExample } from '../providerConfig';
import { runtimeCopy as copy } from '../copy';
</script>
<template>
  <details>
    <summary>{{ copy.models }}</summary>
    <pre>{{ modelConfigExample }}</pre>
  </details>
</template>
<style scoped>
details { font-size: 12px; margin: 10px 0; color: var(--muted); }
pre { white-space: pre-wrap; }
</style>

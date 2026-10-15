<script setup lang="ts">
import { runtimeCopy as copy } from '../copy';
</script>
<template>
  <div role="status">
    <p>{{ copy.contextPriority }}</p>
    <p>{{ copy.contextBlocked }}</p>
  </div>
</template>
<style scoped>
div { padding: 16px; line-height: 1.6; font-size: 13px; }
</style>

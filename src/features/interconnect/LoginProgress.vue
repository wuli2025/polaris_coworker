<script setup lang="ts">
import { interconnectCopy as c } from './copy';
import type { Progress, Stage } from './contracts';
defineProps<{ progress?: Progress | null }>();
const stages: Stage[] = ['register', 'discover', 'relay', 'connect'];
</script>
<template>
  <section v-if="progress" aria-live="polite" class="login-progress">
    <ol>
      <li v-for="stage in stages" :key="stage" :data-stage="stage"
        :aria-current="stage === progress.stage ? 'step' : undefined">
        {{ c.stages[stage] }}
        <span v-if="stage === progress.stage"> — {{ progress.state === 'failed' ? c.failed : progress.state === 'ready' ? c.ready : c.running }}</span>
      </li>
    </ol>
    <p v-if="progress.error" role="alert">{{ progress.error }}</p>
  </section>
</template>

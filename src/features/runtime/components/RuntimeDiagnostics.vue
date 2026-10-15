<script setup lang="ts">
import { computed } from 'vue';
import type { EnvReport, VerifyReport } from '../../../tauri';
import { runtimeRows } from '../presentation';
import { runtimeCopy as copy } from '../copy';
const props = defineProps<{ report: EnvReport; verify: VerifyReport | null }>();
const rows = computed(() => runtimeRows(props.report, props.verify));
</script>
<template>
  <section aria-label="Agent 运行时诊断">
    <p v-if="report.os === 'browser'">{{ copy.preview }}</p>
    <p>后端：{{ report.agentRuntime.backend }} · {{ report.ready && report.shellReady ? copy.ready : copy.notReady }}</p>
    <dl>
      <template v-for="row in rows" :key="row.label">
        <dt>{{ row.label }}</dt><dd>{{ row.value }} <small v-if="row.detail"> · {{ row.detail }}</small></dd>
      </template>
    </dl>
    <p v-if="report.agentRuntime.error">{{ report.agentRuntime.error }}</p>
    <p v-if="!verify?.deep">{{ copy.noInference }}</p>
  </section>
</template>
<style scoped>
section { margin: 16px 0; font-size: 12px; line-height: 1.6; overflow-wrap: anywhere; }
dl { display: grid; grid-template-columns: 100px 1fr; gap: 6px; }
dd { margin: 0; } small { color: var(--muted); }
</style>

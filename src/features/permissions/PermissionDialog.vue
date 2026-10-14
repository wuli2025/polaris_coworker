<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { invoke, listen } from "../../tauri";
import { consume, type Approval, type Decision, type PermissionEvent } from "./queue";

const queue = ref<Approval[]>([]);
const current = computed(() => queue.value[0]);
const busy = ref(false);
const error = ref("");
let dispose: (() => void) | undefined;
let stopped = false;
onMounted(async () => {
  try {
    const stop = await listen<PermissionEvent>("chat:stream", (event) => {
      queue.value = consume(queue.value, event);
    });
    if (stopped) stop(); else dispose = stop;
  } catch { error.value = "无法连接授权请求；未确认的操作会自动拒绝。"; }
});
onUnmounted(() => { stopped = true; dispose?.(); });
async function respond(decision: Decision) {
  const approval = current.value;
  if (!approval || busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    await invoke("chat_permission_respond", { response: {
      sessionId: approval.sessionId, callId: approval.callId, decision,
    } });
    queue.value = queue.value.filter((item) => item !== approval);
  } catch (reason) {
    if (String(reason).includes("APPROVAL_NOT_FOUND") || String(reason).includes("SESSION_NOT_FOUND")) {
      queue.value = queue.value.filter((item) => item !== approval);
    } else error.value = "授权发送失败，请重试；未确认的操作会自动拒绝。";
  } finally { busy.value = false; }
}
</script>

<template>
  <div v-if="current" class="permission-backdrop">
    <section role="dialog" aria-modal="true" aria-labelledby="permission-title" class="permission-dialog">
      <h2 id="permission-title">允许此操作？</h2>
      <p>{{ current.tool }}<span v-if="current.parentCallId"> · 子任务</span></p>
      <pre>{{ current.summary }}</pre>
      <p v-if="error" role="alert">{{ error }}</p>
      <div class="permission-actions">
        <button :disabled="busy" @click="respond('deny')">拒绝</button>
        <button :disabled="busy" @click="respond('allow')">允许一次</button>
        <button :disabled="busy" @click="respond('allowAlways')">本会话相同调用总是允许</button>
      </div>
    </section>
  </div>
</template>

<style scoped>
.permission-backdrop { position: fixed; inset: 0; z-index: 10000; background: #0008; display: grid; place-items: center; }
.permission-dialog { background: var(--bg-primary, #fff); color: var(--text-primary, #222); width: min(90vw, 640px); padding: 24px; border-radius: 12px; }
.permission-dialog h2 { margin: 0 0 12px; font-size: 20px; }
.permission-dialog pre { white-space: pre-wrap; overflow-wrap: anywhere; max-height: 45vh; overflow: auto; padding: 12px; background: #8881; }
.permission-actions { display: flex; flex-wrap: wrap; gap: 8px; justify-content: flex-end; }
.permission-actions button { padding: 8px 12px; border: 1px solid #8886; border-radius: 6px; cursor: pointer; }
</style>

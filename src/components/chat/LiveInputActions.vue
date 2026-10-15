<script setup lang="ts">
import { ref } from 'vue';
import { useChatStore } from '../../stores/chat';
import { sendLiveInput, type LiveInputMode } from './liveInput';

const props = defineProps<{ conversationId: string | null; text: string; disabled?: boolean }>();
const emit = defineEmits<{ sent: [conversationId: string, text: string] }>();
const chat = useChatStore();
const pending = ref(false);
const status = ref('');
async function send(mode: LiveInputMode) {
  const conversationId = props.conversationId;
  const reqId = conversationId ? chat.reqByConv[conversationId] : undefined;
  if (pending.value || props.disabled || !conversationId || !reqId) return;
  const text = props.text;
  pending.value = true;
  status.value = '';
  try {
    await sendLiveInput({ reqId, conversationId }, text, mode);
    chat.pushBubble(conversationId, { role: 'user', text, at: Date.now() });
    status.value = mode === 'steer' ? '插话已接收' : '已排队，将在当前回答后继续';
    emit('sent', conversationId, text);
  } catch (error) {
    status.value = `发送失败，草稿已保留：${String(error)}`;
  } finally { pending.value = false; }
}
</script>

<template>
  <div class="live-input-actions">
    <button type="button" :disabled="pending || disabled || !text.trim()" @click="send('steer')"
      title="在当前任务中补充或纠正要求">插话</button>
    <button type="button" :disabled="pending || disabled || !text.trim()" @click="send('followUp')"
      title="当前回答结束后继续处理这条消息">排队</button>
    <span role="status">{{ status }}</span>
  </div>
</template>

<style scoped>
.live-input-actions { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
button { color: var(--text); background: var(--bg-soft); border: 1px solid var(--border); border-radius: 6px; padding: 4px 8px; cursor: pointer; }
button:disabled { opacity: 0.5; cursor: default; }
span { font-size: 12px; color: var(--text-2); }
</style>

import { computed, onMounted, ref } from 'vue';
import { agentRuntime } from '../../tauri';
import type { AgentBackendStatus } from '../../types/agentRuntime';
import { legacyProviderActions } from './presentation';

export function useBackendStatus() {
  const status = ref<AgentBackendStatus | null>(null);
  const error = ref('');
  async function refreshBackend() {
    try {
      status.value = await agentRuntime.status();
      error.value = '';
    } catch (e) {
      status.value = null;
      error.value = String(e);
    }
  }
  onMounted(refreshBackend);
  return { status, error, refreshBackend,
    legacy: computed(() => legacyProviderActions(status.value)),
    isPi: computed(() => status.value?.backend === 'pi') };
}

import { ref } from 'vue';
import { expertSettings } from './contracts';

export function useExpertSettings() {
  const enabled = ref<boolean | null>(null);
  const busy = ref(false);
  const error = ref('');
  async function refresh() {
    busy.value = true;
    error.value = '';
    try { enabled.value = (await expertSettings.get()).enabled; }
    catch (e) { enabled.value = null; error.value = String(e); }
    finally { busy.value = false; }
  }
  async function set(value: boolean) {
    if (busy.value) return;
    busy.value = true;
    error.value = '';
    try { enabled.value = (await expertSettings.set(value)).enabled; }
    catch (e) {
      enabled.value = null;
      error.value = String(e);
    } finally { busy.value = false; }
  }
  return { enabled, busy, error, refresh, set };
}

<script setup lang="ts">
import { ref } from 'vue';
import { openUrl } from '../../tauri';
import { collabApi, getBase, getToken } from '../collab/api';
import { timing, type RelayConfig } from './contracts';
import { interconnectCopy as c } from './copy';
import { errMsg } from '../../lib/err';
const props = defineProps<{ relay?: RelayConfig }>();
const emit = defineEmits<{ refresh: [] }>();
const open = ref(false);
const urls = ref('');
const address = ref('');
const invite = ref('');
const error = ref('');
const busy = ref(false);
function toggle(event: Event) {
  open.value = (event.target as HTMLDetailsElement).open;
  if (open.value) urls.value = props.relay?.urls?.join('\n') || '';
}
async function openAddress() {
  error.value = '';
  try {
    const url = new URL(address.value.trim());
    if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password || url.search || url.hash) {
      throw new Error(c.invalidAddress);
    }
    await openUrl(url.href);
  } catch (e) { error.value = e instanceof TypeError ? c.invalidAddress : errMsg(e); }
}
async function apply() {
  busy.value = true;
  error.value = '';
  try {
    const relays = urls.value.split(/\s+/).filter(Boolean).map(raw => {
      const url = new URL(raw);
      if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password || url.search || url.hash) {
        throw new Error(c.invalidRelay);
      }
      return { url: raw };
    });
    const response = await fetch(`${getBase()}/api/collab/tunnel/relays`, {
      method: 'POST', headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${getToken()}` },
      body: JSON.stringify({ relays }), signal: AbortSignal.timeout(timing().slowMs),
    });
    const result = await response.json();
    if (!response.ok) throw new Error(result.error || String(response.status));
    emit('refresh');
  } catch (e) { error.value = errMsg(e); }
  finally { busy.value = false; }
}
async function createInvite() {
  busy.value = true;
  error.value = '';
  try {
    const ticket = await collabApi.adminTicket({ role: 'collaborator', note: c.invite });
    if (!ticket.share?.startsWith('PLRS1-')) throw new Error(c.inviteUnavailable);
    invite.value = ticket.share;
  } catch (e) { error.value = errMsg(e); }
  finally { busy.value = false; }
}
</script>
<template>
  <details @toggle="toggle">
    <summary>{{ c.advanced }}</summary>
    <div v-if="open" class="advanced-content">
      <form @submit.prevent="openAddress">
        <label>{{ c.address }}<input v-model="address" type="url" required /></label>
        <button type="submit">{{ c.openAddress }}</button>
      </form>
      <label>{{ c.relayOverride }}<textarea v-model="urls" rows="3" /></label>
      <button :disabled="busy" @click="apply">{{ c.apply }}</button>
      <p v-if="relay">{{ relay.state === 'pending_reconnect' ? c.pendingReconnect : relay.state === 'unavailable' ? c.unavailable : c.relayApplied }}</p>
      <button :disabled="busy" @click="createInvite">{{ c.invite }}</button>
      <p v-if="invite">{{ c.inviteCreated }}：<code>{{ invite }}</code></p>
      <p v-if="error" role="alert">{{ error }}</p>
      <slot />
    </div>
  </details>
</template>
<style scoped>
summary { cursor: pointer; padding: 16px 0; }
label, textarea { display: block; width: 100%; }
code { overflow-wrap: anywhere; }
</style>

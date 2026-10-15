<script setup lang="ts">
import { interconnectCopy as c } from './copy';
import type { Device } from './contracts';
defineProps<{ devices: Device[]; paths: Record<number, { path: string }>; collaboratorNames: string[] }>();
</script>
<template>
  <section class="devices">
    <h2>{{ c.mine }}</h2>
    <p v-if="!devices.length">{{ c.noDevices }}</p>
    <article v-for="device in devices" :key="device.nodeId">
      <strong>{{ device.name }}</strong> <span v-if="device.isSelf">{{ c.self }}</span>
      <p>{{ c.source }}：{{ device.os || c.unknownPlatform }}</p>
      <p>{{ device.online ? c.online : c.offline }} ·
        <span v-if="device.storageMode === 'http' && device.connected">{{ c.httpConnected }}</span>
        <span v-else-if="device.port && paths[device.port]?.path === 'relay'">{{ c.relay }}</span>
        <span v-else-if="device.port && paths[device.port]?.path === 'direct'">{{ c.direct }}</span>
        <span v-else-if="!device.isSelf">{{ c.connecting }}</span>
        · {{ device.storageMode === 'http' ? c.httpStorage : device.mounted ? `${c.mounted} ${device.drive}` : c.unmounted }}
      </p>
      <p v-if="device.error" role="alert">{{ device.error }}</p>
    </article>
    <h2>{{ c.collaborators }}</h2>
    <p v-if="!collaboratorNames.length">{{ c.noCollaborators }}</p>
    <p v-for="name in collaboratorNames" :key="name">{{ name }}</p>
  </section>
</template>
<style scoped>
article { border: 1px solid var(--border, #8884); border-radius: 12px; padding: 14px; margin: 10px 0; }
</style>

<script setup lang="ts">
import { onMounted, ref } from "vue";
import { authed } from "./lib/auth";
import { screen } from "./lib/nav";
import { onWsStatus } from "./lib/net";
import { initChat } from "./lib/chat";
import HostsScreen from "./screens/HostsScreen.vue";
import ChatScreen from "./screens/ChatScreen.vue";
import FilesScreen from "./screens/FilesScreen.vue";
import ProjectsScreen from "./screens/ProjectsScreen.vue";
import SettingsScreen from "./screens/SettingsScreen.vue";
import PlaceholderScreen from "./screens/PlaceholderScreen.vue";
import BottomNav from "./components/BottomNav.vue";
import PreviewOverlay from "./components/PreviewOverlay.vue";
import Toast from "./components/Toast.vue";

const wsOk = ref(false);

onMounted(() => {
  onWsStatus((c) => (wsOk.value = c));
  if (authed.value) initChat();
});
</script>

<template>
  <!-- 第一屏:主机选择(未登录 / 主动切换主机时) -->
  <HostsScreen v-if="!authed || screen === 'hosts'" />
  <template v-else>
    <main class="stage">
      <ChatScreen v-show="screen === 'chat'" :ws-ok="wsOk" />
      <FilesScreen v-if="screen === 'files'" />
      <ProjectsScreen v-if="screen === 'projects'" />
      <SettingsScreen v-if="screen === 'settings'" />
      <PlaceholderScreen
        v-if="screen === 'skills' || screen === 'kb' || screen === 'voice' || screen === 'admin'"
        :which="screen"
      />
    </main>
    <BottomNav />
  </template>
  <PreviewOverlay />
  <Toast />
</template>

<style scoped>
.stage {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
</style>

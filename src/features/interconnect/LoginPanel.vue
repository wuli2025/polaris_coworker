<script setup lang="ts">
import { ref } from 'vue';
import EmailLogin from '../auth/EmailLogin.vue';
import { interconnectCopy as c } from './copy';
defineProps<{ busy: boolean }>();
const emit = defineEmits<{ login: [username: string, password: string]; done: [info: Record<string, unknown>] }>();
const username = ref('');
const password = ref('');
const email = ref(false);
function submit() {
  emit('login', username.value.trim(), password.value);
  password.value = '';
}
</script>
<template>
  <section class="login-panel">
    <form v-if="!email" @submit.prevent="submit">
      <label>{{ c.account }}<input v-model="username" autocomplete="username" required /></label>
      <label>{{ c.password }}<input v-model="password" type="password" autocomplete="current-password" required /></label>
      <button :disabled="busy" type="submit">{{ c.login }}</button>
    </form>
    <EmailLogin v-else @done="emit('done', $event)" />
    <button type="button" :disabled="busy" @click="email = !email">{{ email ? c.passwordLogin : c.emailLogin }}</button>
  </section>
</template>
<style scoped>
form, label { display: grid; gap: 10px; }
.login-panel { max-width: 420px; margin: 24px auto; }
input { padding: 10px; }
button { margin: 10px 0; }
</style>

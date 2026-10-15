import { ref } from 'vue';
import { errMsg } from '../../lib/err';
import { interconnectCopy as c } from './copy';
import type { Command, Device, Progress, RelayConfig } from './contracts';

export interface LoginResult {
  peers?: Device[]; devices?: Device[];
  enrolled?: boolean; token?: string; interconnect?: Progress; relayConfig?: RelayConfig;
  authorityConfigured?: boolean; configurationError?: string;
}
export function useLogin(command: Command, adopt: (token: string) => Promise<void>, slowMs: number) {
  const progress = ref<Progress | null>(null);
  const relay = ref<RelayConfig>();
  const error = ref('');
  const peers = ref<Device[]>([]);
  const busy = ref(false);
  const enrolled = ref(false);
  const authorityConfigured = ref<boolean>();
  let pollBusy = false;
  let disposed = false;
  function receive(result: LoginResult) {
    if (disposed) return;
    if (result.devices || result.peers) peers.value = result.devices || result.peers || [];
    if (result.interconnect) progress.value = result.interconnect;
    if (result.relayConfig) relay.value = result.relayConfig;
    if (result.enrolled !== undefined) enrolled.value = result.enrolled;
    if (result.authorityConfigured !== undefined) authorityConfigured.value = result.authorityConfigured;
    if (result.configurationError) error.value = result.configurationError;
  }
  async function refresh() {
    if (pollBusy || disposed) return;
    pollBusy = true;
    try { receive(await command<LoginResult>('mesh_status')); }
    catch (e) { if (!disposed) error.value = errMsg(e); }
    finally { pollBusy = false; }
  }
  async function done(result: LoginResult) {
    receive(result);
    if (result.token) await adopt(result.token);
  }
  async function execute(action: () => Promise<LoginResult>) {
    if (busy.value || disposed) return;
    busy.value = true;
    error.value = '';
    const timer = setTimeout(() => { if (!disposed) error.value = c.timeout; }, slowMs);
    try {
      const result = await action();
      if (!disposed) error.value = '';
      await done(result);
    }
    catch (e) {
      if (!disposed) {
        error.value = errMsg(e);
        if (progress.value?.state !== 'failed') progress.value = null;
      }
    }
    finally { clearTimeout(timer); busy.value = false; }
  }
  async function login(username: string, password: string) {
    progress.value = null;
    await execute(async () => {
      await command('collab_host_start');
      return command<LoginResult>('account_login_password', { username, password });
    });
  }
  async function retry() {
    if (!enrolled.value || progress.value?.stage === 'register') return;
    await execute(() => command<LoginResult>('mesh_sync'));
    await refresh();
  }
  function dispose() { disposed = true; }
  return { progress, relay, peers, error, busy, enrolled, authorityConfigured, receive, refresh, done, login, retry, dispose };
}

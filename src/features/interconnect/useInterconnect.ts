import { onMounted, onUnmounted, watch, ref } from 'vue';
import { invoke as rawInvoke, isTauri, listen } from '../../tauri';
import { useCollabStore } from '../collab/stores/collab';
import { useAccountStore } from '../../stores/account';
import { useLogin, type LoginResult } from './useLogin';
import { useSecurity } from './useSecurity';
import { timing, type Approval, type Device, type Notice, type Progress, type Timing } from './contracts';
import { interconnectCopy as c } from './copy';
import { errMsg } from '../../lib/err';

const riskCommands = new Set(['peer_grant_set', 'peer_trust', 'peer_revoke', 'mesh_device_remove',
  'mesh_kick', 'mesh_leave', 'fs_unmount', 'fs_share_save', 'fs_share_set']);
export function useInterconnect(
  collab: ReturnType<typeof useCollabStore>, account: ReturnType<typeof useAccountStore>,
  changed: () => Promise<void>, options: Partial<Timing> = {},
) {
  const config = timing(options);
  const login = useLogin(rawInvoke, async token => {
    await collab.adoptLocalSession(token);
    await account.refresh();
    await changed();
  }, config.slowMs);
  const security = useSecurity(rawInvoke);
  const ports = ref<Record<string, number>>({});
  const stops: (() => void)[] = [];
  let disposed = false;
  let poll: ReturnType<typeof setInterval>;
  let retry: ReturnType<typeof setInterval>;
  let polling = false;
  async function refresh() {
    if (!isTauri || polling || disposed) return;
    polling = true;
    try {
      await login.refresh();
      if (collab.authed && login.enrolled.value) {
        await security.refresh();
        const state = await rawInvoke<{ peers?: { nodeId: string; port: number }[] }>('mesh_status');
        if (!disposed) ports.value = Object.fromEntries((state.peers || []).map(p => [p.nodeId, p.port]));
      }
    } catch (e) { if (!disposed) login.error.value = errMsg(e); }
    finally { polling = false; }
  }
  async function passwordLogin(username: string, password: string) {
    if (isTauri) await login.login(username, password);
    else {
      login.busy.value = true;
      login.error.value = '';
      try { await collab.login(username, password); await account.refresh(); }
      catch (e) { login.error.value = errMsg(e); }
      finally { login.busy.value = false; }
    }
    await refresh();
  }
  async function emailDone(info: Record<string, unknown>) {
    await login.done(info as LoginResult);
    await changed();
    await refresh();
  }
  async function logout() {
    await security.run(async () => {
      // Gate before account.logout, which otherwise clears the session even when mesh_leave is pending.
      if (isTauri) await rawInvoke('mesh_leave');
      await collab.logout();
      await account.refresh();
      login.enrolled.value = false;
      login.progress.value = null;
      await changed();
    });
  }
  async function invoke<T>(name: string, args?: Record<string, unknown>): Promise<T> {
    if (!riskCommands.has(name)) return rawInvoke<T>(name, args);
    const payload = args ? JSON.parse(JSON.stringify(args)) as Record<string, unknown> : undefined;
    let result: T | undefined;
    let completed = false;
    await security.run(async () => {
      result = await rawInvoke<T>(name, payload);
      if ((result as { partial?: boolean })?.partial) throw new Error(c.partial);
      completed = true;
      void changed().catch(e => { login.error.value = errMsg(e); });
    });
    if (!completed) throw new Error(security.pending.value ? c.waiting : security.error.value);
    return result as T;
  }
  async function revokeNotice(notice: Notice) {
    const nodeId = notice.nodeId;
    const deviceId = notice.deviceId;
    await security.run(async () => {
      const result = await rawInvoke<{ partial?: boolean }>('mesh_device_remove', { nodeId });
      if (result.partial) throw new Error(c.partial);
      if (!await security.ack(deviceId)) throw new Error(security.error.value);
      await changed();
    });
  }
  async function subscribe<T>(name: string, callback: (event: T) => void) {
    try {
      const stop = await listen<T>(name, callback);
      if (disposed) stop(); else stops.push(stop);
    } catch (e) { if (!disposed) login.error.value = errMsg(e); }
  }
  watch(() => collab.authed, authed => { if (authed) void refresh(); else security.reset(); });
  onMounted(() => {
    if (isTauri) {
      void subscribe<Progress>('interconnect:progress', progress => login.receive({ interconnect: progress }));
      void subscribe<Notice | Approval>('interconnect:security', event => {
        if (!disposed && collab.authed) security.receive(event);
      });
      void refresh();
      poll = setInterval(() => void refresh(), config.pollMs);
      retry = setInterval(() => void security.tick(), config.retryMs);
    }
  });
  onUnmounted(() => {
    disposed = true;
    login.dispose();
    security.reset();
    stops.forEach(stop => stop());
    clearInterval(poll);
    clearInterval(retry);
  });
  function withPorts(devices: Device[]) {
    return devices.filter(d => !d.revoked).map(d => ({ ...d, port: ports.value[d.nodeId] }));
  }
  return { login, security, passwordLogin, emailDone, logout, invoke, revokeNotice, withPorts };
}

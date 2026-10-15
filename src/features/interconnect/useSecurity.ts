import { ref } from 'vue';
import { errMsg } from '../../lib/err';
import { interconnectCopy as c } from './copy';
import type { Approval, Command, Inbox, Notice } from './contracts';

// Retain the exact closure only in memory. Never persist credentials or replay after navigation.
export function useSecurity(command: Command) {
  const notices = ref<Notice[]>([]);
  const approvals = ref<Approval[]>([]);
  const pending = ref('');
  const error = ref('');
  const busy = ref(false);
  const acknowledged = new Set<string>();
  let operation: (() => Promise<unknown>) | undefined;
  let generation = 0;
  let polling = false;

  function pruneApprovals() {
    approvals.value = approvals.value.filter(a => a.expiresAt * 1000 > Date.now());
  }
  function receive(event: Notice | Approval) {
    if ('deviceId' in event && event.kind === 'device_enrolled') {
      if (!acknowledged.has(event.deviceId)) {
        notices.value = [...notices.value.filter(n => n.deviceId !== event.deviceId), event];
      }
    } else if ('id' in event) {
      approvals.value = [...approvals.value.filter(a => a.id !== event.id), event];
      pruneApprovals();
    }
  }
  async function refresh() {
    pruneApprovals();
    if (polling) return;
    polling = true;
    const epoch = generation;
    try {
      const inbox = await command<Inbox>('mesh_security_inbox');
      if (epoch !== generation) return;
      inbox.notifications.forEach(receive);
      approvals.value = inbox.approvals;
      pruneApprovals();
    } catch (e) { if (epoch === generation) error.value = errMsg(e); }
    finally { polling = false; }
  }
  async function ack(deviceId: string) {
    const epoch = generation;
    try {
      await command('mesh_security_ack', { deviceId });
      if (epoch !== generation) return false;
      acknowledged.add(deviceId);
      notices.value = notices.value.filter(n => n.deviceId !== deviceId);
      return true;
    } catch (e) { if (epoch === generation) error.value = errMsg(e); return false; }
  }
  async function decide(requestId: string, approve: boolean) {
    if (busy.value) return;
    busy.value = true;
    const epoch = generation;
    try {
      await command('mesh_risk_decide', { requestId, approve });
      if (epoch === generation) approvals.value = approvals.value.filter(a => a.id !== requestId);
    } catch (e) { if (epoch === generation) error.value = errMsg(e); }
    finally { busy.value = false; }
  }
  async function attempt() {
    if (!operation || busy.value) return;
    busy.value = true;
    const epoch = generation;
    try {
      await operation();
      if (epoch === generation) { operation = undefined; pending.value = ''; error.value = ''; }
    } catch (e) {
      if (epoch !== generation) return;
      const message = errMsg(e);
      const id = message.match(/approval_pending:([A-Za-z0-9_-]+)/)?.[1];
      if (id) { pending.value = id; error.value = ''; }
      else {
        pending.value = '';
        // Explicit user retry is required after denial, expiry, offline failure or partial mutation.
        error.value = message.includes('approval_expired_or_denied:') ? c.expired
          : message.includes(c.offlineRisk) ? c.offlineRisk : message;
      }
    } finally { busy.value = false; }
  }
  async function run(action: () => Promise<unknown>) {
    if (busy.value || pending.value) { error.value = c.pendingOperation; return; }
    operation = action;
    error.value = '';
    await attempt();
  }
  async function tick() { pruneApprovals(); if (pending.value) await attempt(); }
  function cancel() { generation++; operation = undefined; pending.value = ''; error.value = ''; }
  function reset() { cancel(); notices.value = []; approvals.value = []; acknowledged.clear(); }
  return { notices, approvals, pending, error, busy, receive, refresh, ack, decide, run, tick, retry: attempt, cancel, reset };
}

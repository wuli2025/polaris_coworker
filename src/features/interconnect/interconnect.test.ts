import { describe, expect, it, vi } from 'vitest';
import { createSSRApp, h } from 'vue';
import { renderToString } from 'vue/server-renderer';
import { useSecurity } from './useSecurity';
import { useLogin } from './useLogin';
import { timing, type Command, type Notice, type Progress } from './contracts';
import LoginProgress from './LoginProgress.vue';
import SecurityNotices from './SecurityNotices.vue';
import ApprovalPanel from './ApprovalPanel.vue';
import DeviceList from './DeviceList.vue';
import AdvancedPanel from './AdvancedPanel.vue';
import { interconnectCopy as c } from './copy';

vi.mock('../collab/api', () => ({ collabApi: {}, getBase: () => '', getToken: () => '' }));
const notice: Notice = { kind: 'device_enrolled', deviceId: 'dev-new', nodeId: 'node-new', name: 'NAS', platform: 'linux', time: 100 };
const render = (component: Parameters<typeof h>[0], props: Record<string, unknown>) =>
  renderToString(createSSRApp({ render: () => h(component, props) }));

describe('login orchestration', () => {
  it('renders every real stage and failure instead of equating enrolled with ready', async () => {
    for (const stage of ['register', 'discover', 'relay', 'connect'] as const) {
      const progress: Progress = { stage, state: 'running', retryable: false };
      const html = await render(LoginProgress, { progress });
      expect(html).toContain(`data-stage="${stage}" aria-current="step"`);
      expect(html).toContain(c.stages[stage]);
    }
    const progress: Progress = { stage: 'relay', state: 'failed', retryable: true, error: 'relay failed' };
    const command = vi.fn().mockResolvedValueOnce({}).mockResolvedValueOnce({ enrolled: true, token: 'session', interconnect: progress });
    const adopt = vi.fn().mockResolvedValue(undefined);
    const state = useLogin(command as Command, adopt, 30000);
    await state.login('alice', 'secret');
    expect(command.mock.calls[1]).toEqual(['account_login_password', { username: 'alice', password: 'secret' }]);
    expect(adopt).toHaveBeenCalledWith('session');
    expect(state.enrolled.value).toBe(true);
    expect((await render(LoginProgress, { progress: state.progress.value }))).toContain('relay failed');
    command.mockResolvedValue({ interconnect: { stage: 'connect', state: 'ready', retryable: false } });
    await state.retry();
    expect(command.mock.calls.filter(([name]) => name === 'account_login_password')).toHaveLength(1);
    expect(command).toHaveBeenCalledWith('mesh_sync');
    expect(state.progress.value?.state).toBe('ready');
  });
  it('clears the slow-call notice when login eventually completes', async () => {
    vi.useFakeTimers();
    let resolve!: (value: unknown) => void;
    const command = vi.fn().mockResolvedValueOnce({})
      .mockImplementationOnce(() => new Promise(yes => { resolve = yes; }));
    const state = useLogin(command as Command, async () => {}, 1000);
    const request = state.login('alice', 'secret');
    await vi.advanceTimersByTimeAsync(1001);
    expect(state.error.value).toBe(c.timeout);
    resolve({ enrolled: true, token: 'session', interconnect: { stage: 'connect', state: 'ready', retryable: false } });
    await request;
    expect(state.error.value).toBe('');
    expect(state.busy.value).toBe(false);
    expect(state.progress.value?.state).toBe('ready');
    vi.useRealTimers();
  });
  it('exposes a slow call without permitting a duplicate login and releases busy on failure', async () => {
    vi.useFakeTimers();
    let reject!: (reason: string) => void;
    const command = vi.fn().mockImplementation(() => new Promise((_, no) => { reject = no; }));
    const state = useLogin(command as Command, async () => {}, 1000);
    const request = state.login('alice', 'secret');
    await vi.advanceTimersByTimeAsync(1001);
    expect(state.error.value).toBe(c.timeout);
    await state.login('alice', 'secret');
    expect(command).toHaveBeenCalledTimes(1);
    reject('host unavailable');
    await request;
    expect(state.busy.value).toBe(false);
    expect(state.error.value).toBe('host unavailable');
    vi.useRealTimers();
  });
});

describe('policy B', () => {
  it('deduplicates by deviceId, retains failed ack and suppresses late delivery after successful ack', async () => {
    const command = vi.fn().mockRejectedValueOnce('offline').mockResolvedValue({});
    const state = useSecurity(command as Command);
    state.receive(notice);
    state.receive({ ...notice, name: 'NAS renamed' });
    expect(state.notices.value).toHaveLength(1);
    const html = await render(SecurityNotices, { notices: state.notices.value, busy: false });
    expect(html).toContain('NAS renamed');
    expect(html).toContain('linux');
    await state.ack(notice.deviceId);
    expect(state.notices.value).toHaveLength(1);
    await state.ack(notice.deviceId);
    state.receive(notice);
    expect(state.notices.value).toEqual([]);
    expect(command).toHaveBeenLastCalledWith('mesh_security_ack', { deviceId: 'dev-new' });
  });
  it('retries the same operation until approval, executes once and stops replaying after success', async () => {
    const state = useSecurity(vi.fn() as Command);
    const operation = vi.fn().mockRejectedValueOnce('approval_pending:request-1')
      .mockRejectedValueOnce('approval_pending:request-1').mockResolvedValue({ ok: true });
    await state.run(operation);
    expect(state.pending.value).toBe('request-1');
    const html = await render(ApprovalPanel, { approvals: [], pending: state.pending.value, error: '', busy: false });
    expect(html).toContain(c.waiting);
    const competing = vi.fn();
    await state.run(competing);
    expect(competing).not.toHaveBeenCalled();
    await state.tick();
    await state.tick();
    await state.tick();
    await state.retry();
    expect(operation).toHaveBeenCalledTimes(3);
    expect(state.pending.value).toBe('');
  });
  it('requires explicit restart after denial or offline error; unmount cancels pending replay', async () => {
    const state = useSecurity(vi.fn() as Command);
    const operation = vi.fn().mockRejectedValueOnce('approval_pending:r')
      .mockRejectedValueOnce('approval_expired_or_denied:r')
      .mockRejectedValueOnce(c.offlineRisk).mockRejectedValue('approval_pending:r2');
    await state.run(operation);
    await state.tick();
    expect(state.error.value).toBe(c.expired);
    await state.tick();
    expect(operation).toHaveBeenCalledTimes(2);
    await state.retry();
    expect(state.error.value).toBe(c.offlineRisk);
    await state.retry();
    state.reset();
    await state.tick();
    expect(operation).toHaveBeenCalledTimes(4);
  });
  it('decides by request ID and keeps errors visible', async () => {
    const command = vi.fn().mockResolvedValue({});
    const state = useSecurity(command as Command);
    state.receive({ id: 'r', operation: 'delete_account', actor: 'old', host: 'h', expiresAt: Date.now() / 1000 + 60, state: 'pending' });
    expect(state.approvals.value).toHaveLength(1);
    await state.decide('r', false);
    expect(command).toHaveBeenCalledWith('mesh_risk_decide', { requestId: 'r', approve: false });
    expect(state.approvals.value).toEqual([]);
  });
});

describe('default view', () => {
  it('does not mount advanced content until expanded', async () => {
    const html = await renderToString(createSSRApp({ render: () => h(AdvancedPanel, {}, {
      default: () => h('input', { value: 'PLRK1-token-address' }),
    }) }));
    expect(html).toContain('<details ');
    expect(html).not.toContain(' open');
    expect(html).not.toContain('PLRK1');
    expect(html).not.toContain('PLRS1');
    expect(html).not.toContain('<textarea');
    expect(html).not.toContain('<input');
  });
  it('shows actual relay, platform and mount status without exposing credentials', async () => {
    const html = await render(DeviceList, { devices: [{ nodeId: 'n', name: 'NAS', os: 'linux',
      online: true, port: 1234, mounted: true, drive: 'P:', token: 'must-not-render' }],
    paths: { 1234: { path: 'relay' } }, collaboratorNames: ['Alice'] });
    for (const text of ['NAS', 'linux', c.relay, c.mounted, 'P:', 'Alice']) expect(html).toContain(text);
    for (const text of ['PLRK1', 'PLRS1', 'must-not-render', '<input']) expect(html).not.toContain(text);
  });
  it('validates configurable timing', () => {
    expect(timing().retryMs).toBe(5000);
    expect(timing({ retryMs: 10000 }).retryMs).toBe(10000);
    expect(() => timing({ pollMs: 0 })).toThrow();
    expect(() => timing({ retryMs: '5000' as unknown as number })).toThrow();
  });
});

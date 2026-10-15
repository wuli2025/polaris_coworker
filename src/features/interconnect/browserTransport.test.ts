import { describe, expect, it, vi } from 'vitest';
import { createSSRApp, h } from 'vue';
import { renderToString } from 'vue/server-renderer';
import { createInterconnectCommand } from './browserTransport';
import { useLogin } from './useLogin';
import type { Command, Approval } from './contracts';
import ApprovalPanel from './ApprovalPanel.vue';
import { interconnectCopy as c } from './copy';

function fixture(native = false, available = true) {
  const invoke = vi.fn();
  const backend = vi.fn().mockResolvedValue(available);
  const fetch = vi.fn().mockResolvedValue({ ok: true, json: async () => ({}) });
  const command = createInterconnectCommand({ native, invoke: invoke as Command, backend,
    token: () => 'account-session', fetch: fetch as typeof globalThis.fetch });
  return { invoke, backend, fetch, command };
}
describe('environment-selected login transport', () => {
  it('keeps native login and retry on Tauri', async () => {
    const io = fixture(true);
    await io.command('account_login_password', { username: 'a', password: 'b' });
    await io.command('mesh_sync');
    expect(io.invoke.mock.calls).toEqual([['account_login_password', { username: 'a', password: 'b' }], ['mesh_sync', undefined]]);
    expect(io.fetch).not.toHaveBeenCalled();
  });
  it('uses HTTP login then session-authenticated status/retry without invoking server console commands', async () => {
    const io = fixture();
    io.fetch.mockResolvedValue({ ok: true, json: async () => ({ enrolled: true, token: 'new-session',
      interconnect: { stage: 'connect', state: 'failed', retryable: true } }) });
    const adopt = vi.fn();
    const login = useLogin(io.command, adopt, 30000);
    await login.login('alice', 'secret');
    expect(io.fetch.mock.calls[0][0]).toBe('/api/interconnect/login');
    expect(io.fetch.mock.calls[0][1].headers.Authorization).toBeUndefined();
    expect(JSON.parse(io.fetch.mock.calls[0][1].body)).toEqual({ username: 'alice', password: 'secret' });
    expect(adopt).toHaveBeenCalledWith('new-session');
    await login.retry();
    expect(io.fetch.mock.calls.map(([url]) => url)).toEqual([
      '/api/interconnect/login', '/api/interconnect/retry', '/api/interconnect/status',
    ]);
    expect(io.fetch.mock.calls[1][1].headers.Authorization).toBe('Bearer account-session');
    expect(io.invoke).not.toHaveBeenCalled();
  });
  it('retains real HTTP login errors and 429 without falling back to preview', async () => {
    const io = fixture();
    for (const error of ['interconnect_login_failed', 'interconnect_rate_limited']) {
      io.fetch.mockResolvedValue({ ok: false, json: async () => ({ error }) });
      await expect(io.command('account_login_password', { username: 'a', password: 'wrong' })).rejects.toThrow(error);
    }
    expect(io.invoke).not.toHaveBeenCalled();
  });
  it('preview mirrors the progress contract but never adopts a real account', async () => {
    const io = fixture(false, false);
    const adopt = vi.fn();
    const login = useLogin(io.command, adopt, 30000);
    await login.login('a', 'b');
    expect(login.progress.value?.state).toBe('ready');
    expect(login.error.value).toBe(c.preview);
    expect(adopt).not.toHaveBeenCalled();
    expect(io.fetch).not.toHaveBeenCalled();
  });
  it('uses the browser session for notification acknowledgements and approvals', async () => {
    const io = fixture();
    await io.command('mesh_security_ack', { deviceId: 'new' });
    await io.command('mesh_risk_decide', { requestId: 'pending', approve: true });
    expect(io.fetch.mock.calls.map(([url]) => url)).toEqual(['/api/interconnect/security/ack', '/api/interconnect/security/decide']);
    expect(io.fetch.mock.calls[1][1].headers.Authorization).toBe('Bearer account-session');
  });
});

describe('approval device directory presentation', () => {
  const approval: Approval = { id: 'request', actor: 'dev_123', host: 'host', operation: 'delete_account',
    expiresAt: Date.now() / 1000 + 300, state: 'pending' };
  async function render(metadata: Partial<Approval>) {
    return renderToString(createSSRApp({ render: () => h(ApprovalPanel, {
      approvals: [{ ...approval, ...metadata }], pending: '', error: '', busy: false,
    }) }));
  }
  it('shows directory names and platform as escaped text alongside the stable ID', async () => {
    const html = await render({ actorName: '<My laptop>', actorPlatform: 'linux' });
    expect(html).toContain('&lt;My laptop&gt;');
    expect(html).toContain('linux');
    expect(html).toContain('dev_123');
  });
  it('shows placeholders for revoked, unknown and old-server records', async () => {
    for (const metadata of [{}, { actorName: null, actorPlatform: null }]) {
      const html = await render(metadata);
      expect(html).toContain(c.unknownDevice);
      expect(html).toContain(c.unknownPlatform);
      expect(html).toContain('dev_123');
    }
  });
});

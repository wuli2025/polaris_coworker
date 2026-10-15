import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createSSRApp } from 'vue';
import { renderToString } from 'vue/server-renderer';

const state = vi.hoisted(() => ({ authed: false }));
vi.mock('../collab/stores/collab', () => ({ useCollabStore: () => ({
  get authed() { return state.authed; }, user: { username: 'alice', role: 'owner' },
  hostInfo: { running: true, urls: ['http://secret-address'], nodeId: 'node' }, token: 'secret-token',
}) }));
vi.mock('../../stores/account', () => ({ useAccountStore: () => ({ label: 'alice' }) }));
vi.mock('../../stores/app', () => ({ useAppStore: () => ({}) }));
vi.mock('./remoteSources', () => ({ loadRemoteSources: () => [], upsertRemoteSource: vi.fn(), removeRemoteSource: vi.fn() }));
vi.mock('../../tauri', () => ({ isTauri: true, invoke: vi.fn(), listen: vi.fn(), apps: {}, fsmount: {}, openUrl: vi.fn() }));
vi.mock('../collab/api', () => ({ collabApi: {}, parseConnectCode: vi.fn(), getBase: () => '', getToken: () => '' }));
vi.mock('../auth/EmailLogin.vue', () => ({ default: { template: '<form>email login</form>' } }));
vi.mock('../auth/AccountCenterModal.vue', () => ({ default: { template: '<div />' } }));
vi.mock('./RemoteTerminal.vue', () => ({ default: { template: '<div />' } }));
import InterconnectView from './InterconnectView.vue';

describe('actual interconnect default page', () => {
  beforeEach(() => { vi.stubGlobal('navigator', { userAgent: 'test' }); });
  it('shows only account/password login with existing email entry before authentication', async () => {
    state.authed = false;
    const html = await renderToString(createSSRApp(InterconnectView));
    expect(html).toContain('autocomplete="username"');
    expect(html).toContain('type="password"');
    expect(html).toContain('邮箱验证码登录');
    expect(html).not.toContain('我的设备');
    for (const secret of ['连接码', '令牌', 'secret-token', 'secret-address', 'PLRS1', 'PLRK1']) expect(html).not.toContain(secret);
  });
  it('shows devices, collaborators and logout while all legacy controls remain unmounted', async () => {
    state.authed = true;
    const html = await renderToString(createSSRApp(InterconnectView));
    for (const label of ['我的设备', '协作者', '退出登录', '高级']) expect(html).toContain(label);
    for (const secret of ['连接码', '令牌', 'secret-token', 'secret-address', 'PLRS1', 'PLRK1', '<input']) expect(html).not.toContain(secret);
    expect(html).not.toMatch(/<details[^>]*\sopen/);
  });
});

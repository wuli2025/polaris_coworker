import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createPinia, setActivePinia } from 'pinia';
import { previewBackendStatus } from '../../src/types/agentRuntime';
const ports = vi.hoisted(() => ({ status: vi.fn(), list: vi.fn(), link: vi.fn(), route: vi.fn(), proxy: vi.fn() }));
vi.mock('../../src/tauri', () => ({
  agentRuntime: { status: ports.status },
  provider: { list: ports.list, setLinkMode: ports.link, setRouteMode: ports.route, codexProxyInfo: ports.proxy },
}));
import { useProvidersStore } from '../../src/stores/providers';

beforeEach(() => {
  vi.resetAllMocks();
  setActivePinia(createPinia());
  ports.list.mockResolvedValue({ providers: [], currentId: 'test', linkGlobal: false, routeLocal: false });
});
describe('provider store backend routing', () => {
  it('reads backend status and never sends unsupported Pi setters/proxy queries', async () => {
    ports.status.mockResolvedValue(previewBackendStatus);
    const store = useProvidersStore();
    await store.refresh();
    expect(ports.status).toHaveBeenCalledOnce();
    expect(store.legacyActions).toBe(false);
    expect(await store.setLinkMode(true)).toBe(false);
    expect(await store.setRouteMode(true)).toBe(false);
    await store.refreshCodexProxy();
    for (const port of [ports.link, ports.route, ports.proxy]) expect(port).not.toHaveBeenCalled();
  });
  it('retains legacy setters but disables them after status detection fails', async () => {
    ports.status.mockResolvedValue({ ...previewBackendStatus, backend: 'claude-cli' });
    ports.link.mockResolvedValue(true);
    const store = useProvidersStore();
    await store.refresh();
    expect(await store.setLinkMode(true)).toBe(true);
    expect(ports.link).toHaveBeenCalledWith(true);
    ports.status.mockRejectedValue(new Error('offline'));
    await store.refresh();
    expect(store.legacyActions).toBe(false);
    expect(await store.setRouteMode(true)).toBe(false);
    expect(ports.route).not.toHaveBeenCalled();
  });
});

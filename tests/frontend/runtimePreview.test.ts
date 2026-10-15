import { afterEach, describe, expect, it, vi } from 'vitest';
import { agentRuntime, envDoctor, provider } from '../../src/tauri';
import { contextFiles, expertSettings } from '../../src/features/runtime/contracts';

afterEach(() => vi.unstubAllGlobals());
describe('browser runtime preview', () => {
  it('never reports a real runtime, shell or credential as ready', async () => {
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('offline')));
    const status = await agentRuntime.status();
    expect(status).toMatchObject({ backend: 'pi', healthy: false, lastError: 'BrowserPreview' });
    const report = await envDoctor.check();
    expect(report).toMatchObject({ os: 'browser', ready: false, shellReady: false,
      node: { found: false }, agentRuntime: { handshake: false, credentialsConfigured: false, legacyActions: false } });
    expect(await envDoctor.verify(true)).toMatchObject({ deep: false, ok: false });
    await expect(provider.codexStartLogin()).rejects.toThrow('浏览器预览');
    await expect(provider.claudeStartLogin()).rejects.toThrow('浏览器预览');
    expect(await provider.claudeFinishLogin('preview', 'preview', 'preview')).toMatchObject({ loggedIn: false });
    expect(await provider.codexPollLogin('preview', 'preview')).toMatchObject({ status: 'pending' });
    await expect(provider.setLinkMode(true)).rejects.toThrow('Unsupported');
    await expect(provider.setRouteMode(true)).rejects.toThrow('Unsupported');
    await expect(expertSettings.get()).rejects.toThrow('浏览器预览');
    await expect(expertSettings.set(true)).rejects.toThrow('浏览器预览');
    await expect(contextFiles.metadata('project')).rejects.toThrow('浏览器预览');
  });
});

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { fauxAssistantMessage } from '@earendil-works/pi-ai';
import { harness, result } from './helpers.ts';

test('actual expert state defaults off; config.apply affects new and reopened idle sessions', async () => {
  let context = '';
  const capture = (value: unknown) => { context = JSON.stringify(value); return fauxAssistantMessage('done'); };
  const h = await harness([capture, capture, capture, capture]);
  const prompt = async (sessionId: string) => {
    context = '';
    result(await h.request('prompt', { sessionId, text: 'hello' }));
    await h.daemon.sessions.wait(sessionId);
    return context;
  };
  try {
    const state = () => h.request('ping').then(r => result<{ experts: { enabled: boolean } }>(r).experts);
    assert.equal((await state()).enabled, false);
    const options = { cwd: h.dir, expertPrompt: 'request-expert-marker' };
    const { sessionId } = result<{ sessionId: string }>(await h.request('session.open', options));
    assert.ok(!(await prompt(sessionId)).includes('request-expert-marker'));
    result(await h.request('config.apply', { config: { experts: { enabled: true, prompt: 'configured-expert-marker' } } }));
    assert.equal((await state()).enabled, true);
    assert.match(await prompt(sessionId), /request-expert-marker/);
    const fresh = result<{ sessionId: string }>(await h.request('session.open', options));
    const enabledContext = await prompt(fresh.sessionId);
    assert.match(enabledContext, /configured-expert-marker/);
    assert.match(enabledContext, /request-expert-marker/);
    result(await h.request('config.apply', { config: { experts: { enabled: false, prompt: 'configured-expert-marker' } } }));
    assert.equal((await state()).enabled, false);
    const disabled = result<{ sessionId: string }>(await h.request('session.open', options));
    const disabledContext = await prompt(disabled.sessionId);
    assert.ok(!disabledContext.includes('configured-expert-marker'));
    assert.ok(!disabledContext.includes('request-expert-marker'));
    for (const enabled of ['true', 1, null]) {
      const response = await h.request('config.apply', { config: { experts: { enabled, prompt: '' } } });
      assert.equal(response.ok, false);
      if (!response.ok) assert.equal(response.error.code, 'INVALID_CONFIG');
      assert.equal((await state()).enabled, false);
    }
  } finally { await h.close(); }
});

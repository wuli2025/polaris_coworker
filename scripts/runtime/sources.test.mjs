const { test } = await import(process.env.VITEST ? 'vitest' : 'node:test');
import assert from 'node:assert/strict';
import { nodePlan } from './node.mjs';
import { githubUrls } from './sources.mjs';

test('Node uses official, flat R2 and versioned Node mirror without GitHub proxies', () => {
  const plan = nodePlan('22.19.0', 'win32', 'x64', {
    deps: 'https://deps.example.test/packages/', mirror: 'https://mirror.example.test/node///',
  });
  assert.deepEqual(plan.urls, [
    `https://nodejs.org/dist/v22.19.0/${plan.asset}`,
    `https://deps.example.test/packages/${plan.asset}`,
    `https://mirror.example.test/node/v22.19.0/${plan.asset}`,
  ]);
  assert.equal(plan.checksumUrl, 'https://nodejs.org/dist/v22.19.0/SHASUMS256.txt');
  assert.equal(nodePlan('22.19.0', 'linux', 'arm64', { deps: '', mirror: '' }).urls.length, 1);
});

test('runtime sources validate types and HTTPS URL bases', () => {
  for (const value of [null, 123, {}, 'file:///tmp/node', 'http://mirror.test',
    'https://user:password@mirror.test', 'https://mirror.test?path=', 'https://mirror.test/#node']) {
    for (const key of ['deps', 'mirror']) {
      assert.throws(() => nodePlan('22.19.0', 'linux', 'x64', { [key]: value }));
    }
  }
  assert.throws(() => nodePlan('../escape', 'linux', 'x64'));
});

test('GitHub assets retain their own proxy fallback order', () => {
  const official = 'https://github.com/astral-sh/uv/releases/download/v1/uv.zip';
  assert.deepEqual(githubUrls('astral-sh/uv', 'v1', 'uv.zip', { deps: 'https://deps.test/' }), [
    'https://deps.test/uv.zip', `https://ghfast.top/${official}`, `https://ghproxy.net/${official}`, official,
  ]);
});

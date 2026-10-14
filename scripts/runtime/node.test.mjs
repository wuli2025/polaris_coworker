// Keep the same assertions runnable by both required test commands.
const { test } = await import(process.env.VITEST ? 'vitest' : 'node:test');
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { nodeVersion, nodePlan, checksumFor, verifySha256, componentCurrent } from './node.mjs';
test('Node engine minimum and explicit versions are validated', () => {
  assert.equal(nodeVersion('>=22.19.0'), '22.19.0');
  assert.equal(nodeVersion('>=22.19.0', '24.1.0'), '24.1.0');
  for (const v of ['22.18.9', '20.99.0', 'latest', '22.19.0/evil']) {
    assert.throws(() => nodeVersion('>=22.19.0', v));
  }
  assert.throws(() => nodeVersion('^22'));
});
test('official artifacts cover six supported targets and ordered mirrors', () => {
  for (const platform of ['win32', 'darwin', 'linux']) for (const arch of ['x64', 'arm64']) {
    const plan = nodePlan('22.19.0', platform, arch);
    assert.ok(plan.urls[0].startsWith('https://nodejs.org/dist/'));
    assert.ok(plan.urls[1].includes('llmwiki.cloud'));
    assert.ok(plan.urls.every(url => !/ghfast|ghproxy/.test(url)));
    assert.ok(plan.asset.includes(arch));
    assert.equal(plan.check, platform === 'win32' ? 'node.exe' : 'bin/node');
  }
  assert.throws(() => nodePlan('22.19.0', 'linux', 'ia32'));
});
test('checksum requires exact unique asset and rejects corruption and HTML', () => {
  const data = Buffer.from('portable node fixture');
  const hash = createHash('sha256').update(data).digest('hex');
  assert.equal(checksumFor(`${hash}  node.zip\n`, 'node.zip'), hash);
  verifySha256(data, hash);
  assert.throws(() => verifySha256(Buffer.from('<html>proxy failure'), hash));
  assert.throws(() => checksumFor(`${hash}  node.zip.old`, 'node.zip'));
  assert.throws(() => checksumFor(`${hash}  node.zip\n${hash}  node.zip`, 'node.zip'));
});
test('existing binary cannot hide a version or target change', () => {
  const want = {platform: 'win32', arch: 'x64', node: '22.19.0'};
  assert.equal(componentCurrent(want, want, 'node', true), true);
  for (const have of [undefined, {...want, node: '20.0.0'}, {...want, arch: 'arm64'}]) {
    assert.equal(componentCurrent(have, want, 'node', true), false);
  }
  assert.equal(componentCurrent(want, want, 'node', false), false);
});

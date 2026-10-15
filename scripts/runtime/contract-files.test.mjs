import assert from 'node:assert/strict';
const { test } = await import(process.env.VITEST ? 'vitest' : 'node:test');
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import { contractFiles, copyContract } from './contract-files.mjs';

test('packaging follows nested contract helpers and runs without source tree', async () => {
  const root = mkdtempSync(join(tmpdir(), 'pack-contract-'));
  try {
    const source = join(root, 'source');
    const packed = join(root, 'packed');
    mkdirSync(join(source, 'helpers'), { recursive: true });
    writeFileSync(join(source, 'entry.mjs'), 'import { value } from "./helpers/reexport.mjs"; if (value !== 42) throw Error("broken");');
    writeFileSync(join(source, 'helpers/reexport.mjs'), 'export { value } from "./value.mjs";');
    writeFileSync(join(source, 'helpers/value.mjs'), 'export const value = 42;');
    writeFileSync(join(source, 'unrelated.mjs'), 'throw Error("must not ship");');
    assert.deepEqual(contractFiles(source, 'entry.mjs'), ['entry.mjs', 'helpers/reexport.mjs', 'helpers/value.mjs']);
    copyContract(source, 'entry.mjs', packed);
    rmSync(source, { recursive: true });
    execFileSync(process.execPath, [join(packed, 'entry.mjs')], { stdio: 'inherit' });
    assert.match(readFileSync(join(packed, 'helpers/value.mjs'), 'utf8'), /42/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('current SDK contract includes its helper in the source graph', () => {
  const root = fileURLToPath(new URL('../../agentd/', import.meta.url));
  assert.ok(contractFiles(root, 'test/sdk-contract.test.ts').includes('test/synchronization.ts'));
});

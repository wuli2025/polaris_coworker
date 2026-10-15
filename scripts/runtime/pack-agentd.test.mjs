import assert from 'node:assert/strict';
import { existsSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { packAgentd } from '../pack-agentd.mjs';
const runner = await import(process.env.VITEST ? 'vitest' : 'node:test');

async function packageAndVerify() {
  const root = mkdtempSync(join(tmpdir(), 'pack-agentd-regression-'));
  const output = join(root, 'agentd');
  try {
    // Includes build, production install, contract imports, WASM and native addon checks.
    await packAgentd(output);
    assert.ok(existsSync(join(output, 'dist/src/main.js')));
    assert.ok(existsSync(join(output, '.stamp.json')));
    assert.equal(existsSync(join(output, '.contract-tests')), false);
    assert.equal(existsSync(join(output, 'dist/test')), false);
    assert.equal(existsSync(`${output}.staging`), false);
  } finally { rmSync(root, { recursive: true, force: true }); }
}
if (process.env.VITEST) runner.test('minimal production package passes and removes tests', packageAndVerify, 120000);
else runner.test('minimal production package passes and removes tests', { timeout: 120000 }, packageAndVerify);

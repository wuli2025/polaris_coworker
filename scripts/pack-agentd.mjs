// Target-native packaging: npm selects optional dependencies for the build host.
import { cpSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync, renameSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { execFileSync } from 'node:child_process';
import { nodeVersion } from './runtime/node.mjs';
import { contractFiles, copyContract } from './runtime/contract-files.mjs';
const repo = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const source = join(repo, 'agentd');
function files(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap(e => {
    const path = join(dir, e.name);
    return e.isDirectory() ? files(path) : [path];
  }).sort();
}
function bytes(dir) { return files(dir).reduce((n, p) => n + statSync(p).size, 0); }
export async function packAgentd(destination = join(repo, 'src-tauri/runtime/agentd')) {
  nodeVersion(JSON.parse(readFileSync(join(source, 'package.json'))).engines.node, process.versions.node);
  const hash = createHash('sha256').update(`${process.platform}/${process.arch}/${process.versions.node}`);
  for (const path of ['package.json', 'package-lock.json', 'tsconfig.json', ...contractFiles(source, 'test/sdk-contract.test.ts'), ...files(join(source, 'src')),
    ...files(join(source, 'config')), fileURLToPath(import.meta.url),
    join(repo, 'scripts/runtime/contract-files.mjs'), join(repo, 'scripts/runtime/node.mjs'), join(repo, 'scripts/runtime/check-wasm.mjs')]) {
    hash.update(path).update(readFileSync(resolve(source, path)));
  }
  const digest = hash.digest('hex');
  const stamp = join(destination, '.stamp.json');
  let previous;
  try { previous = JSON.parse(readFileSync(stamp)); } catch { /* missing or corrupt stamp: rebuild */ }
  if (previous?.digest === digest
    && existsSync(join(destination, 'dist/src/main.js')) && existsSync(join(destination, 'node_modules/.package-lock.json'))) {
    console.log(`[pack-agentd] current: ${bytes(destination)} bytes`);
    return;
  }
  const staging = `${destination}.staging`;
  rmSync(staging, { recursive: true, force: true });
  mkdirSync(staging, { recursive: true });
  const npm = process.platform === 'win32' ? 'npm.cmd' : 'npm';
  const run = (args, cwd) => execFileSync(npm, args, { cwd, stdio: 'inherit', shell: process.platform === 'win32' });
  run(['run', 'build'], source);
  for (const name of ['package.json', 'package-lock.json', 'config']) cpSync(join(source, name), join(staging, name), { recursive: true });
  cpSync(join(source, 'dist/src'), join(staging, 'dist/src'), { recursive: true });
  run(['ci', '--omit=dev', '--ignore-scripts', '--no-audit', '--no-fund'], staging);
  // Verify the installed production tree, without tsx or TypeScript in the payload.
  const testDir = join(staging, '.contract-tests');
  mkdirSync(testDir);
  try {
    copyContract(join(source, 'dist'), 'test/sdk-contract.test.js', testDir);
    execFileSync(process.execPath, ['--test', join(testDir, 'test/sdk-contract.test.js')], { cwd: staging, stdio: 'inherit' });
    execFileSync(process.execPath, [join(repo, 'scripts/runtime/check-wasm.mjs'), staging], { stdio: 'inherit' });
  } finally { rmSync(testDir, { recursive: true, force: true }); }
  const composition = Object.fromEntries(['dist', 'config', 'node_modules'].map(name => [name, bytes(join(staging, name))]));
  writeFileSync(join(staging, '.stamp.json'), JSON.stringify({ digest, platform: process.platform, arch: process.arch, composition }, null, 2));
  rmSync(destination, { recursive: true, force: true });
  renameSync(staging, destination);
  console.log(`[pack-agentd] ${bytes(destination)} bytes: ${JSON.stringify(composition)}`);
}
if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) await packAgentd(process.argv[2]);

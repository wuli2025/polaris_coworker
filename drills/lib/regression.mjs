import { readdir, readFile } from 'node:fs/promises';
import { join, extname, resolve, relative } from 'node:path';
import { Blocked, check, json, repo } from './harness.mjs';
export function validateConfig(c) {
  for (const list of ['suites', 'builds']) {
    if (!Array.isArray(c[list]) || !c[list].length) throw new Blocked(`regression ${list} must be a nonempty list`);
    const ids = new Set();
    for (const s of c[list]) {
      if (!/^[a-z][a-z0-9-]*$/.test(s.id) || ids.has(s.id)) throw new Blocked('Invalid or duplicate suite id');
      ids.add(s.id);
      if (typeof s.cwd !== 'string' || !safeRelative(s.cwd)) throw new Blocked('Suite cwd must be repository relative');
      if (!Array.isArray(s.command) || !s.command.length || s.command.some(v => typeof v !== 'string' || !v || v.includes('\0'))) {
        throw new Blocked('Suite command must be a nonempty argv list');
      }
    }
  }
  if (c.builds.length !== 3) throw new Blocked('Exactly three builds required');
  for (const key of ['maxLines', 'maxColumns']) if (!Number.isSafeInteger(c[key]) || c[key] < 1 || c[key] > 500) throw new Blocked(`${key} must be 1..500`);
  if (!Array.isArray(c.lineRoots) || !c.lineRoots.length || c.lineRoots.some(s => typeof s !== 'string' || !safeRelative(s))) throw new Blocked('Invalid lineRoots');
  if (!Array.isArray(c.extensions) || !c.extensions.length || c.extensions.some(s => typeof s !== 'string' || !/^\.[a-z]+$/.test(s))) throw new Blocked('Invalid extensions');
  return c;
}
function safeRelative(s) { return !s.startsWith('/') && !s.split(/[\\/]/).includes('..'); }
export async function regressionConfig() {
  let value;
  try { value = await json(process.env.DRILL_REGRESSION_CONFIG ?? join(repo, 'drills/config/regression.json')); }
  catch { throw new Blocked('Regression configuration missing or invalid JSON'); }
  return validateConfig(value);
}
export async function rustSuites() {
  const suites = [];
  for (const dir of await readdir(join(repo, 'src-tauri/crates'), { withFileTypes: true })) {
    if (!dir.isDirectory()) continue;
    const manifest = await readFile(join(repo, 'src-tauri/crates', dir.name, 'Cargo.toml'), 'utf8');
    const name = manifest.match(/^name\s*=\s*"([^"]+)"/m)?.[1];
    check(name, 'Crate manifest has no name');
    const features = name === 'polaris-collab' ? ['--features', 'collab-host,collab-net'] : [];
    suites.push({ id: `crate-${name}`, cwd: 'src-tauri', command: ['cargo', 'test', '-p', name, ...features] });
  }
  return suites;
}
export function warnings(text) {
  const lines = text.replace(/\x1b\[[0-9;]*m/g, '').split('\n');
  return lines.filter(line => /(^|\s)(warning(?:\[|:)|WARN\b)|\(!\)/i.test(line)
    && !/generated \d+ warning|duplicate\)/.test(line)).length;
}
export function summary(rows) {
  const pass = rows.filter(r => r.status === 'pass').length;
  const fail = rows.filter(r => r.status === 'fail').length;
  const blocked = rows.filter(r => r.status === 'blocked').length;
  return { attempts: rows.length, pass, fail, blocked, passRate: rows.length ? pass / rows.length : 0 };
}
export async function lineCheck(config, root = repo) {
  const violations = []; let files = 0;
  async function visit(path) {
    for (const e of await readdir(path, { withFileTypes: true })) {
      if (['node_modules', 'target', '.git', 'dist', '__pycache__'].includes(e.name)) continue;
      const f = join(path, e.name);
      if (e.isDirectory()) await visit(f);
      else if (e.isFile() && config.extensions.includes(extname(e.name))) {
        const source = await readFile(f, 'utf8');
        const lines = source.split('\n'); if (lines.at(-1) === '') lines.pop();
        files++;
        const columns = Math.max(0, ...lines.map(line => [...line.replace(/\r$/, '')].length));
        if (lines.length > config.maxLines || columns > config.maxColumns) violations.push({ file: relative(root, f), lines: lines.length, columns });
      }
    }
  }
  for (const path of config.lineRoots) await visit(resolve(root, path));
  return { files, violations };
}

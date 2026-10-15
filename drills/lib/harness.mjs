import { mkdtemp, mkdir, rm, writeFile, appendFile, readFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { randomUUID } from 'node:crypto';
import { setTimeout as delay } from 'node:timers/promises';
export const repo = fileURLToPath(new URL('../../', import.meta.url));
export class Blocked extends Error {}
export function check(ok, message) {
  if (!ok) throw Object.assign(new Error(message), { safeReason: message });
}
export function options(args, steps) {
  const out = { keep: false, only: null, repeat: 5 };
  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--keep') out.keep = true;
    else if (args[i] === '--only') out.only = args[++i];
    else if (args[i] === '--repeat') out.repeat = Number(args[++i]);
    else throw new Blocked('Unknown argument; use --only STEP, --keep, --repeat N');
  }
  if (out.only !== null && !steps.includes(out.only)) throw new Blocked(`Unknown step; available: ${steps.join(', ')}`);
  if (!Number.isSafeInteger(out.repeat) || out.repeat < 1 || out.repeat > 100) throw new Blocked('repeat must be 1..100');
  return out;
}
export function integer(env, key, fallback, min = 1, max = 3600000) {
  const value = env[key] === undefined ? fallback : Number(env[key]);
  if (!Number.isSafeInteger(value) || value < min || value > max) throw new Blocked(`${key} must be an integer ${min}..${max}`);
  return value;
}
export async function waitFor(probe, { timeoutMs = 30000, intervalMs = 100, label = 'readiness' } = {}) {
  const end = performance.now() + timeoutMs;
  while (true) {
    const result = await probe();
    if (result) return result;
    const remaining = end - performance.now();
    if (remaining <= 0) throw new Error(`Timed out waiting for ${label}`);
    await delay(Math.min(intervalMs, remaining));
  }
}
// Only explicit public diagnostics enter JSONL. Never include response bodies or raw Error messages.
export class Harness {
  static async create(drill, steps, args = process.argv.slice(2)) {
    const [major, minor] = process.versions.node.split('.').map(Number);
    if (process.platform !== 'linux') throw new Blocked('Drills require Linux; use WSL2/Linux for real runs');
    if (major < 22 || (major === 22 && minor < 19)) throw new Blocked('Node >=22.19 required');
    const h = new Harness();
    h.drill = drill; h.options = options(args, steps); h.rows = []; h.cleanup = [];
    h.timeout = integer(process.env, 'DRILL_TIMEOUT_MS', 180000);
    h.commandTimeout = integer(process.env, 'DRILL_COMMAND_TIMEOUT_MS', 1800000, 1000, 86400000);
    h.root = await mkdtemp('/tmp/polaris-drill-');
    h.id = `p7-${randomUUID().slice(0, 12)}`;
    h.home = join(h.root, 'home'); h.logs = join(h.root, 'logs');
    h.secrets = join(h.root, 'secrets');
    for (const path of [h.home, h.logs, h.secrets]) await mkdir(path, { recursive: true, mode: 0o700 });
    h.secretValues = new Set();
    return h;
  }
  redact(text) {
    let out = String(text);
    for (const value of this.secretValues) if (value) out = out.split(value).join('[REDACTED]');
    out = out.replace(/(\"(?:access|refresh|key|password|token|assertion|deviceAssertion|meshKey)\"\s*:\s*\")[^\"]*/g, '$1[REDACTED]');
    return out.replace(/(Bearer\s+)\S+/gi, '$1[REDACTED]');
  }
  async log(name, text) { await appendFile(join(this.logs, `${name.replaceAll('/', '-')}.log`), this.redact(text), { mode: 0o600 }); }
  async emit(step, status, evidence, durationMs = 0) {
    const row = { drill: this.drill, step, status, evidence, durationMs: Math.round(durationMs) };
    this.rows.push(row);
    const line = `${JSON.stringify(row)}\n`;
    await appendFile(join(this.root, 'results.jsonl'), line, { mode: 0o600 });
    process.stdout.write(line);
    return row;
  }
  async step(name, work) {
    const started = performance.now();
    try { return await this.emit(name, 'pass', await work(), performance.now() - started); }
    catch (error) {
      const blocked = error instanceof Blocked;
      // Assertion messages are in code; arbitrary tool/API error text is suppressed.
      const reason = blocked ? error.message : (error.safeReason ?? 'Assertion or command failed; inspect redacted logs');
      await this.log(name, `${error.stack ?? error}\n`);
      return this.emit(name, blocked ? 'blocked' : 'fail', { reason, log: `${name.replaceAll('/', '-')}.log`, ...(error.evidence ?? {}) }, performance.now() - started);
    }
  }
  async finish() {
    this.finishing ??= this.finishOnce();
    return this.finishing;
  }
  async finishOnce() {
    let failed = this.rows.some(r => r.status !== 'pass');
    for (const cleanup of this.cleanup.reverse()) {
      try { await cleanup(); } catch { failed = true; await this.emit('cleanup', 'fail', { reason: 'Resource cleanup failed; retained directory', root: this.root }); }
    }
    // Credentials, refreshed tokens, session text, CA keys and backups NEVER survive --keep.
    for (const path of [this.home, this.secrets]) {
      try { await rm(path, { recursive: true, force: true }); }
      catch { failed = true; await this.emit('cleanup-sensitive', 'fail', { reason: 'Could not delete generated sensitive data; manual cleanup required', root: this.root }); }
    }
    if (failed && this.options.keep) await this.emit('artifacts', 'pass', { root: this.root, credentialsRemoved: !this.rows.some(r => r.step === 'cleanup-sensitive') });
    else {
      try { await rm(this.root, { recursive: true, force: true }); }
      catch { await this.emit('cleanup-directory', 'fail', { root: this.root, reason: 'Temporary directory removal failed' }); }
    }
    return this.rows.some(r => r.status === 'fail') ? 1 : this.rows.some(r => r.status === 'blocked') ? 2 : 0;
  }
}
export async function run(drill, steps, setup = async () => {}) {
  let h;
  try {
    h = await Harness.create(drill, Object.keys(steps));
    for (const [signal, code] of [['SIGINT', 130], ['SIGTERM', 143], ['SIGHUP', 129]]) {
      process.once(signal, async () => { h.cancelled = true; await h.finish(); process.exit(code); });
    }
    const selected = h.options.only ? [h.options.only] : Object.keys(steps);
    for (const name of selected) await h.step(name, async () => { await setup(h, name); return steps[name](h); });
  } catch (error) {
    if (h) await h.emit('setup', 'blocked', { reason: error instanceof Blocked ? error.message : 'Harness initialization failed' });
    else process.stdout.write(`${JSON.stringify({ drill, step: 'setup', status: 'blocked', evidence: { reason: error instanceof Blocked ? error.message : 'Harness initialization failed' }, durationMs: 0 })}\n`);
  } finally { process.exitCode = h ? await h.finish() : 2; }
}
export async function json(path) { return JSON.parse(await readFile(path, 'utf8')); }
export async function save(path, value) { await writeFile(path, JSON.stringify(value, null, 2), { mode: 0o600 }); }
export function childEnv(h, extra = {}) {
  // Deliberately no inherited model keys, POLARIS_*, NODE_OPTIONS or user data paths.
  // Proxy settings are network routing, not secrets: drills must exercise the product's proxy support.
  const proxy = Object.fromEntries(['HTTP_PROXY', 'HTTPS_PROXY', 'NO_PROXY', 'http_proxy', 'https_proxy', 'no_proxy']
    .filter((key) => process.env[key]).map((key) => [key, process.env[key]]));
  return { PATH: process.env.PATH, LANG: 'C.UTF-8', HOME: h.home, TMPDIR: h.secrets, ...proxy,
    XDG_CONFIG_HOME: join(h.home, '.config'), XDG_CACHE_HOME: join(h.home, '.cache'),
    XDG_DATA_HOME: join(h.home, '.local/share'), ...extra };
}

import { cp, mkdir, readdir, lstat, readFile, writeFile, realpath, chmod } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { randomBytes, createHash } from 'node:crypto';
import { homedir } from 'node:os';
import { createServer } from 'node:net';
import { settings } from './settings.mjs';
import { Api, request } from './api.mjs';
import { Blocked, check, childEnv, integer, json, repo, save, waitFor } from './harness.mjs';
import { command, start, ownedAgent } from './process.mjs';
export function buildEnv(h) {
  return childEnv(h, { CARGO_HOME: process.env.CARGO_HOME ?? join(homedir(), '.cargo'),
    RUSTUP_HOME: process.env.RUSTUP_HOME ?? join(homedir(), '.rustup') });
}
export async function port() {
  const socket = createServer();
  await new Promise((resolve, reject) => socket.once('error', reject).listen(0, '127.0.0.1', resolve));
  const value = socket.address().port;
  await new Promise(resolve => socket.close(resolve));
  return value;
}
export async function build(h) {
  if (h.built) return h.built;
  h.built = (async () => {
    h.agentEntry = join(h.root, 'runtime/agentd/dist/src/main.js');
    await command(h, 'build-agentd', process.execPath, ['scripts/pack-agentd.mjs', join(h.root, 'runtime/agentd')], { env: buildEnv(h) });
    await command(h, 'build-web', 'npm', ['run', 'build'], { env: buildEnv(h) });
    await command(h, 'build-server', 'cargo', ['build', '--release', '--manifest-path', 'src-tauri/Cargo.toml',
      // Same target as the Dockerfile: the server binary lives in polaris-cli.
      '-p', 'polaris-cli', '--bin', 'polaris-server', '--features', 'collab-net'],
    { env: buildEnv(h) });
    return { agentd: 'production package', server: 'release/server,collab-net', frontend: 'production' };
  })();
  return h.built;
}
async function validateTree(path, privatize = false) {
  const stat = await lstat(path);
  if (stat.isSymbolicLink() || (!stat.isDirectory() && !stat.isFile())) throw new Blocked('Credential source must contain only regular files/directories, no symlinks');
  if (privatize) await chmod(path, stat.isDirectory() ? 0o700 : 0o600);
  if (stat.isDirectory()) for (const name of await readdir(path)) await validateTree(join(path, name), privatize);
}
function collectSecrets(h, value) {
  if (typeof value === 'string') h.secretValues.add(value);
  else if (value && typeof value === 'object') for (const v of Object.values(value)) collectSecrets(h, v);
}
export async function credentials(h) {
  const source = process.env.DRILL_PI_AGENT_DIR;
  if (!source) throw new Blocked('Set DRILL_PI_AGENT_DIR to an explicit staging copy of the logged-in Pi agent directory');
  if (!source.startsWith('/')) throw new Blocked('DRILL_PI_AGENT_DIR must be an absolute POSIX staging directory');
  const actual = await realpath(source).catch(() => { throw new Blocked('Credential staging directory is unavailable'); });
  // The staging copy is the sole permitted external data input; never traverse a user profile.
  if (/^\/mnt\/[a-z]\/Users\//i.test(actual) || actual === homedir() || actual.startsWith(`${homedir()}/Polaris`)) {
    throw new Blocked('Stage credentials outside Windows profiles and ~/Polaris before running');
  }
  await validateTree(source);
  const copy = join(h.home, '.pi/agent');
  await cp(source, copy, { recursive: true, dereference: false });
  await validateTree(copy, true);
  let auth;
  try { auth = await json(join(copy, 'auth.json')); } catch { throw new Blocked('Pi staging copy needs valid auth.json'); }
  check(auth && typeof auth === 'object' && !Array.isArray(auth), 'Invalid auth.json');
  collectSecrets(h, auth);
  const provider = process.env.DRILL_PROVIDER ?? 'openai-codex';
  if (!['openai-codex', 'anthropic'].includes(provider)) throw new Blocked('DRILL_PROVIDER supports openai-codex or anthropic');
  if (!auth[provider] || !['oauth', 'api_key'].includes(auth[provider].type)) throw new Blocked('Selected provider has no supported credential in copied auth.json');
  h.provider = provider === 'anthropic' ? 'claude-official' : 'codex';
  h.model = process.env.DRILL_MODEL;
  if (h.model !== undefined && (!h.model.trim() || h.model.length > 200)) throw new Blocked('DRILL_MODEL must be a nonempty model id <=200 characters');
  const data = join(h.home, 'Polaris/data');
  await mkdir(data, { recursive: true });
  await save(join(data, 'providers.json'), { current_id: h.provider, link_global: false, route_local: false, items: [], credentials: auth });
}
export async function server(h) {
  if (h.started) return h.started;
  h.started = (async () => {
    const profile = await settings(h);
    await credentials(h);
    await build(h);
    const config = join(h.root, 'agent-config.json');
    await save(config, { agent: { backend: 'pi' }, agentd: {
      node: process.execPath, entry: h.agentEntry, isolation: 'shared',
      sessionMap: join(h.home, 'session-map'), expertSettings: join(h.home, 'expert-settings.json'),
      ...profile.host,
    } });
    await save(join(h.root, 'daemon.json'), { sessionDir: join(h.home, 'sessions'), skills: { paths: [] },
      permissions: { defaultMode: 'manual', timeoutMs: integer(process.env, 'DRILL_PERMISSION_TIMEOUT_MS', 3000), summaryChars: 2000 } });
    h.token = randomBytes(24).toString('hex'); h.secretValues.add(h.token);
    const listen = await port();
    const executable = join(repo, 'src-tauri/target/release/polaris-server');
    h.server = start(h, 'server', executable, [], { env: childEnv(h, {
      POLARIS_PROVIDER_STORE: join(h.home, 'Polaris/data/providers.json'),
      POLARIS_AGENT_CONFIG: config, POLARIS_AGENTD_CONFIG: join(h.root, 'daemon.json'),
      POLARIS_AUTH_TOKEN: h.token, POLARIS_PORT: String(listen), POLARIS_WEB_DIR: join(repo, 'dist'),
      POLARIS_TUNNEL_AUTOSTART: '0',
    }) });
    h.api = new Api(h, `http://127.0.0.1:${listen}`, h.token);
    await waitFor(async () => {
      check(h.server.child.exitCode === null, 'server exited before ready');
      try { return (await request(h.api.base, '/api/ready')).status === 200; } catch { return false; }
    }, { timeoutMs: h.timeout, label: 'HTTP readiness' });
    await h.api.connect();
    await waitFor(async () => (await h.api.invoke('agent_backend_status')).healthy,
      { timeoutMs: h.timeout, label: 'Pi supervisor healthy' });
    if (h.model) await setModel(h, h.model);
  })();
  return h.started;
}
export async function setModel(h, model, contextWindow) {
  const metadata = contextWindow ? { id: model, contextWindow, maxTokens: Math.min(4096, contextWindow) } : model;
  await h.api.invoke('provider_save', { input: { id: h.provider, name: 'Drill subscription', settingsConfig: { models: [metadata] } } });
  await h.api.invoke('provider_switch', { id: h.provider });
}
export async function conversation(h, suffix = '') {
  const project = await h.api.invoke('conv_create_project', { name: `Drill ${suffix}` });
  const cwd = join(h.home, `project-${project.id}`);
  await mkdir(cwd, { recursive: true });
  await h.api.invoke('conv_set_project_work_dir', { projectId: project.id, workDir: cwd });
  const conv = await h.api.invoke('conv_create_conversation', { projectId: project.id });
  check(typeof conv.id === 'string', 'Missing conversation id');
  return { id: conv.id, projectId: project.id, cwd };
}
export async function mapping(h, id) {
  return json(join(h.home, 'session-map', `${createHash('sha256').update(id).digest('hex')}.json`));
}
export async function memory(h) {
  const status = await h.api.invoke('agent_backend_status');
  return { agentdRssKiB: await ownedAgent(h, status.pid) };
}
export async function sessionFiles(dir) {
  const out = [];
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) out.push(...await sessionFiles(path));
    else if (entry.name.endsWith('.jsonl')) out.push(path);
  }
  return out;
}

export async function sessionEntries(h, id) {
  const mapped = await mapping(h, id);
  for (const file of await sessionFiles(join(h.home, 'sessions'))) {
    const entries = (await readFile(file, 'utf8')).split('\n').filter(Boolean).map(line => JSON.parse(line));
    if (entries[0]?.id === mapped.id) return entries;
  }
  throw new Error('Persisted session file missing');
}

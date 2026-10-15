import { mkdir, readFile, writeFile, access, symlink, chmod } from 'node:fs/promises';
import { join } from 'node:path';
import { run, check, Blocked, childEnv, repo } from './lib/harness.mjs';
import { server, conversation, mapping, sessionEntries } from './lib/agent.mjs';
import { respondDuring } from './lib/permissions.mjs';
import { docker, dockerAvailable, assertSandbox, cleanupContainer } from './lib/docker.mjs';
import { settings } from './lib/settings.mjs';
import { start } from './lib/process.mjs';
const exists = path => access(path).then(() => true, () => false);
async function permission(h, mode, decision) {
  const c = await conversation(h, `${mode}-${decision}`);
  const target = join(mode === 'auto_all' ? h.home : c.cwd, `permission-${c.id}.txt`);
  const t = await h.api.begin(c.id, `Use the write tool once to create ${target} containing DRILL_OK. If denied, stop; do not try alternative tools.`, { permissionMode: mode });
  respondDuring(h.api, t, () => decision === 'timeout' ? null : decision);
  const r = await h.api.end(t, mode === 'deny' || decision !== 'allow');
  if (mode === 'manual') {
    check(r.events.some(e => e.kind === 'permission.request'), 'No manual permission request');
    const resolutions = r.events.filter(e => e.kind === 'permission.resolved').map(e => JSON.parse(e.text).reason);
    check(resolutions.includes(decision === 'timeout' ? 'expired' : 'responded'), 'Missing permission resolution');
  }
  const allowed = mode !== 'deny' && decision === 'allow';
  check(await exists(target) === allowed, 'Permission outcome did not match filesystem');
  if (allowed) check((await readFile(target, 'utf8')).includes('DRILL_OK'), 'Wrong file content');
  if (mode.startsWith('auto_')) check(!r.events.some(e => e.kind === 'permission.request'), 'Automatic mode unexpectedly requested permission');
  if (mode === 'deny') check(!(await mapping(h, c.id)).tools.includes('write'), 'deny profile exposes write');
  return { mode, decision, fileCreated: allowed, ...r.metrics };
}
const steps = {
  'manual-allow': h => permission(h, 'manual', 'allow'),
  'manual-deny': h => permission(h, 'manual', 'deny'),
  'manual-timeout': h => permission(h, 'manual', 'timeout'),
  deny: h => permission(h, 'deny', 'deny'),
  'auto-current': h => permission(h, 'auto_current', 'allow'),
  'auto-all': h => permission(h, 'auto_all', 'allow'),
  async traversal(h) {
    const c = await conversation(h, 'traversal');
    const outside = join(h.home, 'outside');
    await mkdir(outside); await symlink(outside, join(c.cwd, 'escape'));
    const results = [];
    for (const path of ['escape/../escape/blocked.txt', '../outside/blocked.txt']) {
      const t = await h.api.begin(c.id, `Use write exactly once with path ${path} and content ESCAPED. Do not normalize the path or retry.`, { permissionMode: 'auto_current' });
      respondDuring(h.api, t, () => 'deny');
      const r = await h.api.end(t, true);
      check(r.events.some(e => e.kind === 'permission.request' && JSON.parse(JSON.parse(e.text).summary).path === path), 'Exact traversal path was not submitted for permission');
      check(r.events.some(e => e.kind === 'error'), 'No rejected tool event');
      check(!await exists(join(outside, 'blocked.txt')), 'Escaped project boundary');
      results.push({ path, denied: true });
    }
    return { results, policy: 'auto-current external writes require approval; drill denies via API' };
  },
  async docker(h) {
    const policy = (await settings(h)).sandbox;
    await dockerAvailable(h);
    const project = join(h.home, 'docker-project'), kb = join(h.home, 'docker-kb');
    await mkdir(project); await chmod(project, 0o777); await mkdir(kb);
    await writeFile(join(kb, 'readonly.txt'), 'KB');
    await mkdir(join(h.home, 'Polaris/data'), { recursive: true });
    await writeFile(join(h.home, 'Polaris/data', 'sentinel.key'), 'DRILL_FAKE_SECRET');
    const name = `${h.id}-sandbox`;
    const image = `${h.id}-sandbox-image`;
    await docker(h, 'sandbox-build', ['build', '-t', image, '-f',
      join(repo, 'src-tauri/crates/polaris-sandbox/src/templates/Dockerfile.sandbox'), repo]);
    h.cleanup.push(() => docker(h, 'remove-sandbox-image', ['image', 'rm', image]));
    cleanupContainer(h, name);
    await docker(h, 'sandbox-run', ['run', '-d', '--name', name, `--memory=${policy.memoryBytes}`, `--cpus=${policy.cpus}`,
      '--cap-drop=ALL', '--security-opt=no-new-privileges',
      '--mount', `type=bind,src=${project},dst=/workspace`, '--mount', `type=bind,src=${kb},dst=/kb,readonly`, image, 'sleep', 'infinity']);
    const info = JSON.parse((await docker(h, 'sandbox-inspect', ['inspect', name])).text)[0];
    assertSandbox(info, project, kb);
    await docker(h, 'sandbox-visible', ['exec', name, 'sh', '-ec',
      'ls /workspace /kb; test ! -e "$HOME/Polaris/data"; test ! -e /home/polaris/Polaris/data; test -z "$(find /workspace /kb "$HOME" -name \"*.key\" -print)"; echo OK > /workspace/visible.txt']);
    check((await readFile(join(project, 'visible.txt'), 'utf8')).trim() === 'OK', 'Project mount not writable');
    const ro = await start(h, 'sandbox-readonly', 'docker', ['exec', name, 'sh', '-c', 'echo BAD > /kb/readonly.txt'], { env: childEnv(h) }).done();
    check(ro.code !== 0 && (await readFile(join(kb, 'readonly.txt'), 'utf8')) === 'KB', 'KB is writable');
    const failed = await start(h, 'sandbox-nonzero', 'docker', ['exec', name, 'sh', '-c', 'exit 37'], { env: childEnv(h) }).done();
    check(failed.code === 37, 'Docker lost command exit code');
    return { mounts: info.Mounts.map(m => ({ destination: m.Destination, writable: m.RW })), capDrop: info.HostConfig.CapDrop,
      securityOpt: info.HostConfig.SecurityOpt, failureExitCode: failed.code, scope: 'container policy fixture; not product executor' };
  },
  async 'server-unavailable'(h) {
    const c = await conversation(h, 'sandbox-unavailable');
    const t = await h.api.begin(c.id, 'Use bash to write HOST_FALLBACK into fallback.txt. Stop after any error.', { useSandbox: true, permissionMode: 'auto_all' });
    const r = await h.api.end(t, true);
    check(r.events.some(e => e.kind === 'error'), 'Sandbox absence did not report an error');
    const persisted = await sessionEntries(h, c.id).catch(() => []);
    check(JSON.stringify([...r.events, ...persisted]).includes('SANDBOX_UNAVAILABLE'), 'Error was not a sandbox-unavailable failure');
    check(!await exists(join(c.cwd, 'fallback.txt')), 'Sandbox fell back to host');
    return { hostFallback: false, ...r.metrics, scope: 'server has no sandbox executor' };
  },
  async 'docker-down'() {
    throw new Blocked('Server has no SandboxExecutor (src-tauri/src/wiring.rs). A dedicated daemon stop/restart cannot exercise product Docker routing until this is wired; never stop the host shared Docker daemon');
  },
  async 'protected-project'() {
    throw new Blocked('Product mount validation requires desktop SandboxPolicy; server wiring omits it. A fixture-only symlink check would not prove product rejection');
  },
};
await run('D3', steps, async (h, name) => {
  if (!['docker', 'docker-down', 'protected-project'].includes(name)) await server(h);
});

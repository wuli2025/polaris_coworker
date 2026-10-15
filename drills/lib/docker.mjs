import { join } from 'node:path';
import { command, start } from './process.mjs';
import { childEnv, Blocked, check } from './harness.mjs';
export async function docker(h, name, args) {
  return command(h, name, 'docker', args, { env: childEnv(h, { DOCKER_CONFIG: join(h.secrets, 'docker') }) });
}
export async function dockerAvailable(h) {
  try { await docker(h, 'docker-info', ['info', '--format', '{{json .ServerVersion}}']); }
  catch { throw new Blocked('Local Docker daemon unavailable; use an isolated real Linux Docker host'); }
}
export function cleanupContainer(h, name) {
  h.cleanup.push(async () => {
    const result = await start(h, `remove-${name}`, 'docker', ['rm', '-fv', name],
      { env: childEnv(h, { DOCKER_CONFIG: join(h.secrets, 'docker') }) }).done(30000);
    // A never-created container is harmless; other failures must surface.
    check(result.code === 0 || result.text.includes('No such container'), 'Container cleanup failed');
  });
}
export function assertSandbox(info, project, kb) {
  check(info.Mounts?.length === 2, 'Expected exactly two mounts');
  for (const [destination, source, rw] of [['/workspace', project, true], ['/kb', kb, false]]) {
    const m = info.Mounts.find(m => m.Destination === destination);
    check(m?.Type === 'bind' && m.Source === source && m.RW === rw, 'Sandbox mount mismatch');
  }
  const c = info.HostConfig;
  check(c.CapDrop?.includes('ALL'), 'Missing CapDrop ALL');
  check(c.SecurityOpt?.some(s => ['no-new-privileges', 'no-new-privileges:true'].includes(s)), 'Missing no-new-privileges');
  check(!c.Privileged && c.PidMode !== 'host' && c.NetworkMode !== 'host' && c.IpcMode !== 'host', 'Unsafe namespaces');
  check(!c.Devices?.length && !c.CapAdd?.length, 'Extra devices or capabilities');
  check(c.Memory > 0 && c.Memory <= 4294967296 && c.NanoCpus > 0 && c.NanoCpus <= 2000000000, 'Resource limits missing');
}

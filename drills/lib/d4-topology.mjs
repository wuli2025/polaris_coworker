import { mkdir, writeFile, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { randomBytes, createHash } from 'node:crypto';
import { settings } from './settings.mjs';
import { command, start } from './process.mjs';
import { buildEnv, port } from './agent.mjs';
import { docker, dockerAvailable } from './docker.mjs';
import { request } from './api.mjs';
import { childEnv, repo, save, waitFor, check, Blocked } from './harness.mjs';
export async function compose(h, name, args) {
  return docker(h, name, ['compose', '-p', h.id, '-f', join(h.root, 'compose.json'), ...args]);
}
export async function fixture(h, ...args) {
  return command(h, 'fixture', 'python3', [join(repo, 'drills/lib/fixture.py'), ...args], { env: buildEnv(h) });
}
export async function topology(h) {
  if (h.topology) return h.topology;
  h.topology = (async () => {
    const policy = (await settings(h)).interconnect;
    await dockerAvailable(h);
    h.images = { server: `${h.id}-server:drill`, relay: `${h.id}-relay:drill` };
    // Images have unique tags; only remove tags successfully built by this invocation.
    for (const [name, file] of [['server', 'Dockerfile'], ['relay', 'deploy/singapore/Dockerfile.relay']]) {
      await docker(h, `build-${name}`, ['build', '-t', h.images[name], '-f', join(repo, file), repo]);
      h.cleanup.push(() => docker(h, `remove-${name}-image`, ['image', 'rm', h.images[name]]));
    }
    h.password = randomBytes(24).toString('hex'); h.secretValues.add(h.password);
    const ca = join(h.secrets, 'ca'); await mkdir(ca);
    await command(h, 'ca-create', 'openssl', ['req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-days', '1',
      '-subj', '/CN=Polaris Drill CA', '-keyout', join(ca, 'ca.key'), '-out', join(ca, 'ca.crt')]);
    await command(h, 'cert-request', 'openssl', ['req', '-newkey', 'rsa:2048', '-nodes', '-subj', '/CN=authority.drill',
      '-keyout', join(ca, 'server.key'), '-out', join(ca, 'server.csr')]);
    await writeFile(join(ca, 'extensions'), 'subjectAltName=DNS:authority.drill,DNS:relay.drill\nextendedKeyUsage=serverAuth\n');
    await command(h, 'cert-sign', 'openssl', ['x509', '-req', '-days', '1', '-in', join(ca, 'server.csr'),
      '-CA', join(ca, 'ca.crt'), '-CAkey', join(ca, 'ca.key'), '-CAcreateserial', '-extfile', join(ca, 'extensions'), '-out', join(ca, 'server.crt')]);
    h.data = {};
    for (const name of ['authority', 'member-a', 'member-b', 'new-device']) {
      h.data[name] = join(h.secrets, name);
      await mkdir(join(h.data[name], 'Polaris/data'), { recursive: true });
      await mkdir(join(h.data[name], 'share'), { recursive: true });
    }
    await command(h, 'seed-account', 'cargo', ['run', '--manifest-path', 'src-tauri/Cargo.toml', '-p', 'polaris-collab',
      '--features', 'collab-host,collab-net', '--example', 'seed_authority_account', '--', 'drill-user', h.password, 'registered@example.invalid', 'Drill'],
    { env: { ...buildEnv(h), POLARIS_COLLAB_DB: join(h.data.authority, 'Polaris/data/collab.db') } });
    await fixture(h, 'owner', join(h.data.authority, 'Polaris/data/collab.db'));
    h.pollSeconds = policy.pollSeconds;
    const baseConfig = { relay: { urls: ['https://relay.drill'], fallback: false },
      security: { revocationPollSecs: h.pollSeconds, revocationMaxAgeSecs: policy.maxAgeSeconds, newDeviceWindowHours: policy.windowHours, approvalTimeoutSecs: policy.approvalSeconds },
      rateLimit: { windowSecs: 60, readRequests: 120, writeRequests: 30, loginRequests: policy.loginRequests, globalMultiplier: 20 } };
    await save(join(h.root, 'authority.json'), baseConfig);
    // TLS is terminated by Caddy, as in deploy/singapore; no public DNS/ACME needed.
    await writeFile(join(h.root, 'Caddyfile'), '{\n auto_https off\n}\nauthority.drill {\n tls /ca/server.crt /ca/server.key\n reverse_proxy authority:8080\n}\nrelay.drill {\n tls /ca/server.crt /ca/server.key\n reverse_proxy relay:8080\n}\n');
    await writeFile(join(h.root, 'relay.toml'), 'http_bind_addr = "0.0.0.0:8080"\nenable_relay = true\nenable_quic_addr_discovery = false\nenable_metrics = false\naccess = "everyone"\n');
    const services = {};
    h.ports = {};
    for (const name of Object.keys(h.data)) {
      h.ports[name] = await port();
      const authority = name === 'authority';
      services[name] = { image: h.images.server, entrypoint: ['/usr/local/bin/polaris-server'],
        user: `${process.getuid()}:${process.getgid()}`, restart: 'no', security_opt: ['no-new-privileges:true'],
        ports: [`127.0.0.1:${h.ports[name]}:8080`],
        environment: { HOME: '/home/polaris', POLARIS_PORT: '8080', POLARIS_WEB_DIR: '/srv/web',
          POLARIS_ACCOUNT_AUTHORITY: authority ? '1' : '0', POLARIS_AUTHORITY_ONLY: authority ? '1' : '0',
          POLARIS_REQUIRE_LOGIN: '1', POLARIS_TUNNEL_AUTOSTART: '0', POLARIS_ACCOUNT_OPEN_SIGNUP: '0',
          POLARIS_EMAIL_SIGNUP: '1', POLARIS_SMTP_HOST: '127.0.0.1', POLARIS_SMTP_PORT: '1',
          POLARIS_SMTP_USER: 'drill@example.invalid', POLARIS_SMTP_PASS: 'fake-local-only',
          POLARIS_INTERCONNECT_CONFIG: '/etc/polaris/interconnect.json',
          POLARIS_FS_ROOTS: '/home/polaris/share', POLARIS_FS_WRITE: '1' },
        volumes: [`${h.data[name]}:/home/polaris`, `${join(ca, 'ca.crt')}:/tmp/drill-ca.crt:ro`, `${join(h.root, `${authority ? 'authority' : 'member'}.json`)}:/etc/polaris/interconnect.json:ro`],
      };
    }
    services.relay = { image: h.images.relay, command: ['--config-path', '/etc/iroh/relay.toml'],
      volumes: [`${join(h.root, 'relay.toml')}:/etc/iroh/relay.toml:ro`], restart: 'no', security_opt: ['no-new-privileges:true'] };
    services.tls = { image: policy.caddyImage, restart: 'no', networks: { default: { aliases: ['authority.drill', 'relay.drill'] } },
      volumes: [`${join(h.root, 'Caddyfile')}:/etc/caddy/Caddyfile:ro`, ...['server.crt', 'server.key'].map(f => `${join(ca, f)}:/ca/${f}:ro`)] };
    await save(join(h.root, 'compose.json'), { services });
    // Register cleanup before first create; no resource survives even a partial startup.
    h.cleanup.push(() => compose(h, 'compose-clean', ['down', '--volumes', '--remove-orphans', '--timeout', '10']));
    await compose(h, 'authority-start', ['up', '-d', 'authority', 'relay', 'tls']);
    const publicKey = await waitFor(async () => {
      try { const r = await request(`http://127.0.0.1:${h.ports.authority}`, '/api/account/pubkey'); return r.status === 200 ? r.value : false; }
      catch { return false; }
    }, { timeoutMs: h.timeout, label: 'authority public key' });
    h.fingerprint = createHash('sha256').update(Buffer.from(publicKey.publicKey, 'base64url')).digest('hex');
    check(publicKey.fingerprint === h.fingerprint, 'Authority fingerprint does not match its public key');
    await save(join(h.root, 'member.json'), { ...baseConfig, authority: { url: 'https://authority.drill', publicKeyFingerprints: [h.fingerprint], extraCaPem: '/tmp/drill-ca.crt' } });
    await compose(h, 'members-start', ['up', '-d', 'member-a', 'member-b', 'new-device']);
    for (const name of Object.keys(h.data)) await waitFor(async () => {
      try { return (await request(`http://127.0.0.1:${h.ports[name]}`, '/api/ready')).status === 200; } catch { return false; }
    }, { timeoutMs: h.timeout, label: `${name} ready` });
    // curl explicitly trusts the generated CA. This does NOT prove ureq/iroh trust it.
    await compose(h, 'tls-probe', ['exec', '-T', 'authority', 'curl', '--cacert', '/tmp/drill-ca.crt', '-fsS', '-o', '/dev/null', 'https://authority.drill/api/account/pubkey']);
    await compose(h, 'relay-probe', ['exec', '-T', 'authority', 'curl', '--cacert', '/tmp/drill-ca.crt', '-fsS', '-o', '/dev/null', 'https://relay.drill/generate_204']);
    return { containers: Object.keys(services), fingerprint: h.fingerprint, ca: 'temporary', relayQuicDiscovery: false };
  })();
  return h.topology;
}
export async function accountCall(h, path, body, token, service = 'authority') {
  const r = await request(`http://127.0.0.1:${h.ports[service]}`, path, body, token);
  for (const key of ['token', 'assertion', 'deviceAssertion', 'meshKey']) if (typeof r.value?.[key] === 'string') h.secretValues.add(r.value[key]);
  return r;
}
export async function login(h, name = 'Old device', service = 'authority') {
  const r = await accountCall(h, '/api/interconnect/login', { username: 'drill-user', password: h.password, deviceName: name, platform: 'web' }, undefined, service);
  check(r.status === 200 && r.value.enrolled === true && r.value.interconnect?.state === 'ready', 'Password login/enrollment did not become ready');
  return r.value;
}
export function delegatedBlocked() {
  throw new Blocked('This step still requires explicit member admission and real host enrollment, not web_ session identities. CA trust is exercised separately by the tls step. See docs/drills/P7a-blockers.md');
}

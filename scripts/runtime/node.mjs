import { createHash } from 'node:crypto';

export function nodeVersion(engine, override) {
  const match = /^>=(\d+\.\d+\.\d+)$/.exec(engine);
  if (!match) throw new Error(`Unsupported Node engine: ${engine}`);
  const version = override ?? match[1];
  if (!/^\d+\.\d+\.\d+$/.test(version)) throw new Error('Node version must be x.y.z');
  const a = version.split('.').map(Number), b = match[1].split('.').map(Number);
  const difference = a.map((n, i) => n - b[i]).find(n => n !== 0) ?? 0;
  if (difference < 0) throw new Error(`Node ${version} does not satisfy ${engine}`);
  return version;
}

export function nodePlan(version, platform, arch, sources = {}) {
  if (!['win32', 'darwin', 'linux'].includes(platform) || !['x64', 'arm64'].includes(arch)) {
    throw new Error(`Unsupported Node target: ${platform}/${arch}`);
  }
  const win = platform === 'win32';
  const asset = `node-v${version}-${win ? 'win' : platform}-${arch}.${win ? 'zip' : 'tar.gz'}`;
  const official = `https://nodejs.org/dist/v${version}`;
  const base = sources.deps ?? 'https://llmwiki.cloud/downloads/deps';
  const proxies = sources.proxies ?? ['https://ghfast.top', 'https://ghproxy.net'];
  for (const url of [base, ...proxies]) {
    if (new URL(url).protocol !== 'https:') throw new Error('Runtime sources must use HTTPS');
  }
  return {
    name: 'node', dir: 'node', asset, magic: win ? '504b' : '1f8b',
    minBytes: 10 * 1024 * 1024, strip: 1, check: win ? 'node.exe' : 'bin/node',
    urls: [`${base}/${asset}`, ...proxies.map(p => `${p}/${official}/${asset}`), `${official}/${asset}`],
    checksumUrl: `${official}/SHASUMS256.txt`,
  };
}

export function checksumFor(manifest, asset) {
  const matches = manifest.split(/\r?\n/).map(line => /^([a-fA-F0-9]{64})\s+\*?(\S+)$/.exec(line))
    .filter(match => match?.[2] === asset);
  if (matches.length !== 1) throw new Error(`Missing or duplicate official SHA256 for ${asset}`);
  return matches[0][1].toLowerCase();
}

export function verifySha256(bytes, expected) {
  if (!/^[a-f0-9]{64}$/.test(expected) || createHash('sha256').update(bytes).digest('hex') !== expected) {
    throw new Error('Node SHA256 mismatch');
  }
}

export function componentCurrent(have, want, name, intact) {
  return intact && have?.platform === want.platform && have?.arch === want.arch && have?.[name] === want[name];
}

// GitHub release proxies cannot serve nodejs.org assets.
export const defaultDepsBase = 'https://llmwiki.cloud/downloads/deps';

function baseUrl(value) {
  if (typeof value !== 'string') throw new TypeError('Runtime source must be a URL string');
  const url = new URL(value);
  if (url.protocol !== 'https:' || url.username || url.password || url.search || url.hash) {
    throw new Error('Runtime sources must use HTTPS without credentials, query or fragment');
  }
  return value.replace(/\/+$/, '');
}

export function nodeUrls(version, asset, { deps = defaultDepsBase, mirror } = {}) {
  const official = `https://nodejs.org/dist/v${version}/${asset}`;
  return [official, ...(deps === '' ? [] : [`${baseUrl(deps)}/${asset}`]),
    ...(mirror === undefined || mirror === '' ? [] : [`${baseUrl(mirror)}/v${version}/${asset}`])];
}

export function githubUrls(repo, tag, asset, { deps = defaultDepsBase } = {}) {
  const official = `https://github.com/${repo}/releases/download/${tag}/${asset}`;
  return [...(deps === '' ? [] : [`${baseUrl(deps)}/${asset}`]),
    `https://ghfast.top/${official}`, `https://ghproxy.net/${official}`, official];
}

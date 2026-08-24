#!/usr/bin/env node

import { readFile, readdir, writeFile } from "node:fs/promises";
import {
  createHash,
  createPublicKey,
  verify as verifyEd25519,
} from "node:crypto";
import { basename, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const REQUIRED_PLATFORMS = [
  "windows-x86_64",
  "darwin-x86_64",
  "darwin-aarch64",
];

const HELP = `Usage:
  node scripts/release-manifest.mjs \\
    --artifacts ARTIFACT_DIR \\
    --version VERSION \\
    --repo OWNER/REPOSITORY \\
    --pub-date ISO_DATE \\
    --out OUTPUT_FILE \\
    [--remote-base HTTPS_BASE_URL]

Generates a deterministic Tauri latest.json from signed Windows and macOS artifacts.
--remote-base additionally verifies that the deployed installer bytes match local size and SHA-256.
`;

const ED25519_SPKI_PREFIX = Buffer.from("302a300506032b6570032100", "hex");

function decodeBase64(value, label) {
  const encoded = value.trim();
  if (
    encoded.length === 0 ||
    encoded.length % 4 === 1 ||
    !/^[A-Za-z0-9+/]+={0,2}$/.test(encoded)
  ) {
    throw new Error(`${label} is not valid base64`);
  }
  const decoded = Buffer.from(encoded, "base64");
  if (decoded.toString("base64").replace(/=+$/, "") !== encoded.replace(/=+$/, "")) {
    throw new Error(`${label} is not canonical base64`);
  }
  return decoded;
}

function decodeUtf8(bytes, label) {
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    throw new Error(`${label} is not valid UTF-8`);
  }
}

function meaningfulLines(value) {
  return value.replace(/\r\n/g, "\n").replace(/\n+$/, "").split("\n");
}

function decodePublicKey(encodedPublicKey) {
  assert(
    typeof encodedPublicKey === "string" && encodedPublicKey.trim().length > 0,
    "Tauri updater minisign public key is missing",
  );
  const text = decodeUtf8(
    decodeBase64(encodedPublicKey, "Tauri updater minisign public key"),
    "Tauri updater minisign public key",
  );
  const lines = meaningfulLines(text);
  assert(lines.length === 2, "Tauri updater minisign public key has an invalid format");
  assert(
    lines[0].startsWith("untrusted comment: "),
    "Tauri updater minisign public key has no untrusted comment",
  );
  const payload = decodeBase64(lines[1], "Tauri updater minisign public key payload");
  assert(payload.length === 42, "Tauri updater minisign public key payload has an invalid length");
  assert(
    payload.subarray(0, 2).toString("ascii") === "Ed",
    "Tauri updater minisign public key uses an unsupported algorithm",
  );
  return {
    keyId: payload.subarray(2, 10),
    key: createPublicKey({
      key: Buffer.concat([ED25519_SPKI_PREFIX, payload.subarray(10)]),
      format: "der",
      type: "spki",
    }),
  };
}

export function verifyMinisignSignature(bytes, encodedSignature, encodedPublicKey) {
  assert(Buffer.isBuffer(bytes), "signed artifact bytes are missing");
  assert(
    typeof encodedSignature === "string" && encodedSignature.trim().length > 0,
    "minisign signature is missing",
  );
  const publicKey = decodePublicKey(encodedPublicKey);
  const text = decodeUtf8(
    decodeBase64(encodedSignature, "minisign signature"),
    "minisign signature",
  );
  const lines = meaningfulLines(text);
  assert(lines.length === 4, "minisign signature has an invalid format");
  assert(
    lines[0].startsWith("untrusted comment: "),
    "minisign signature has no untrusted comment",
  );
  assert(
    lines[2].startsWith("trusted comment: "),
    "minisign signature has no trusted comment",
  );

  const signaturePacket = decodeBase64(lines[1], "minisign signature packet");
  assert(signaturePacket.length === 74, "minisign signature packet has an invalid length");
  const algorithm = signaturePacket.subarray(0, 2).toString("ascii");
  assert(algorithm === "Ed" || algorithm === "ED", "minisign signature uses an unsupported algorithm");
  assert(
    signaturePacket.subarray(2, 10).equals(publicKey.keyId),
    "minisign signature key ID does not match the updater public key",
  );

  const rawSignature = signaturePacket.subarray(10);
  const message = algorithm === "ED" ? createHash("blake2b512").update(bytes).digest() : bytes;
  assert(
    verifyEd25519(null, message, publicKey.key, rawSignature),
    "minisign artifact signature verification failed",
  );

  const globalSignature = decodeBase64(lines[3], "minisign global signature");
  assert(globalSignature.length === 64, "minisign global signature has an invalid length");
  const trustedComment = Buffer.from(lines[2].slice("trusted comment: ".length), "utf8");
  assert(
    verifyEd25519(
      null,
      Buffer.concat([rawSignature, trustedComment]),
      publicKey.key,
      globalSignature,
    ),
    "minisign trusted-comment signature verification failed",
  );
}

async function filesUnder(root) {
  const files = [];
  async function visit(directory) {
    const entries = await readdir(directory, { withFileTypes: true });
    entries.sort((left, right) => left.name.localeCompare(right.name));
    for (const entry of entries) {
      const path = join(directory, entry.name);
      if (entry.isDirectory()) await visit(path);
      else if (entry.isFile()) files.push(path);
    }
  }
  await visit(root);
  return files;
}

async function releaseInput(path, signaturePath, label) {
  if (!signaturePath) throw new Error(`${label} signature is missing`);
  const signature = (await readFile(signaturePath, "utf8")).trim();
  if (!signature) throw new Error(`${label} signature is empty`);
  const bytes = await readFile(path);
  return {
    filename: basename(path),
    path,
    signature,
    size: bytes.length,
    magic: bytes.subarray(0, 8),
    bytes,
    sha256: createHash("sha256").update(bytes).digest("hex"),
  };
}

export async function findReleaseInputs(root) {
  const artifactRoot = resolve(root);
  const files = await filesUnder(artifactRoot);
  const windows = files.filter((path) => /Polaris_[^/\\]+_x64-setup\.exe$/.test(path));
  const mac = files.filter((path) => basename(path) === "Polaris.app.tar.gz");
  if (windows.length !== 1) {
    throw new Error(`expected exactly one Windows updater installer, found ${windows.length}`);
  }
  if (mac.length !== 1) {
    throw new Error(`expected exactly one macOS updater archive, found ${mac.length}`);
  }
  const signatureFor = (path) => files.find((candidate) => candidate === `${path}.sig`);
  return {
    windows: await releaseInput(
      windows[0],
      signatureFor(windows[0]),
      "Windows updater",
    ),
    mac: await releaseInput(mac[0], signatureFor(mac[0]), "macOS updater"),
  };
}

function proxiedGithubAsset(repo, version, filename) {
  const githubAsset = `https://github.com/${repo}/releases/download/v${version}/${encodeURIComponent(filename)}`;
  return `https://gh-proxy.com/${githubAsset}`;
}

export function buildReleaseManifest({ version, repo, pubDate, inputs }) {
  const windows = {
    signature: inputs.windows.signature,
    url: proxiedGithubAsset(repo, version, inputs.windows.filename),
  };
  const mac = {
    signature: inputs.mac.signature,
    url: proxiedGithubAsset(repo, version, inputs.mac.filename),
  };
  return {
    version,
    notes: `Polaris v${version}`,
    pub_date: pubDate,
    platforms: {
      "windows-x86_64": windows,
      "darwin-x86_64": mac,
      "darwin-aarch64": { ...mac },
    },
  };
}

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

export function validateReleaseManifest(manifest, { version, repo, pubDate, inputs, publicKey }) {
  assert(manifest && typeof manifest === "object", "manifest must be an object");
  assert(manifest.version === version, `manifest version must be ${version}`);
  assert(
    typeof pubDate === "string" && Number.isFinite(Date.parse(pubDate)),
    `pub-date is not a valid ISO date: ${pubDate}`,
  );
  assert(manifest.pub_date === pubDate, "manifest pub_date changed during generation");
  assert(/^[^/\s]+\/[^/\s]+$/.test(repo), `invalid GitHub repo: ${repo}`);
  const platforms = manifest.platforms;
  assert(platforms && typeof platforms === "object", "manifest platforms are missing");
  const keys = Object.keys(platforms);
  assert(
    keys.length === REQUIRED_PLATFORMS.length &&
      REQUIRED_PLATFORMS.every((platform) => keys.includes(platform)),
    `manifest platforms must be exactly ${REQUIRED_PLATFORMS.join(", ")}`,
  );
  assert(
    inputs.windows.filename.includes(`_${version}_`),
    `Windows installer filename does not contain version ${version}`,
  );

  const allowedFilenames = new Set([inputs.windows.filename, inputs.mac.filename]);
  for (const platform of REQUIRED_PLATFORMS) {
    const entry = platforms[platform];
    assert(entry && typeof entry === "object", `${platform} entry is missing`);
    assert(
      typeof entry.signature === "string" && entry.signature.trim().length > 0,
      `${platform} signature is empty`,
    );
    assert(typeof entry.url === "string" && entry.url.startsWith("https://"), `${platform} URL must use HTTPS`);
    assert(
      (entry.url.match(/https:\/\/github\.com\//g) || []).length === 1,
      `${platform} URL must contain exactly one GitHub release URL`,
    );
    const filename = decodeURIComponent(new URL(entry.url).pathname.split("/").at(-1));
    assert(allowedFilenames.has(filename), `${platform} URL references unknown artifact ${filename}`);
  }
  assert(
    platforms["windows-x86_64"].signature === inputs.windows.signature,
    "Windows signature does not match its artifact",
  );
  for (const platform of ["darwin-x86_64", "darwin-aarch64"]) {
    assert(
      platforms[platform].signature === inputs.mac.signature,
      `${platform} signature does not match the universal macOS artifact`,
    );
  }
  for (const [label, input] of [
    ["Windows updater", inputs.windows],
    ["macOS updater", inputs.mac],
  ]) {
    try {
      verifyMinisignSignature(input.bytes, input.signature, publicKey);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      throw new Error(`${label} minisign verification failed: ${message}`);
    }
  }
}

function remoteBaseAllowed(url) {
  if (url.protocol === "https:") return true;
  return (
    url.protocol === "http:" &&
    ["127.0.0.1", "localhost", "::1", "[::1]"].includes(url.hostname)
  );
}

export async function verifyRemoteAssets(inputs, remoteBase) {
  const base = new URL(remoteBase.endsWith("/") ? remoteBase : `${remoteBase}/`);
  assert(remoteBaseAllowed(base), "remote-base must use HTTPS (HTTP is only allowed for loopback tests)");
  for (const input of [inputs.windows, inputs.mac]) {
    const url = new URL(encodeURIComponent(input.filename), base);
    const response = await fetch(url, { redirect: "follow" });
    if (!response.ok) throw new Error(`${input.filename} returned HTTP ${response.status}`);
    const bytes = Buffer.from(await response.arrayBuffer());
    if (bytes.length !== input.size) {
      throw new Error(`${input.filename} length mismatch: remote ${bytes.length}, local ${input.size}`);
    }
    if (!bytes.subarray(0, 8).equals(input.magic)) {
      throw new Error(`${input.filename} magic bytes do not match the signed local artifact`);
    }
    const remoteSha256 = createHash("sha256").update(bytes).digest("hex");
    if (remoteSha256 !== input.sha256) {
      throw new Error(`${input.filename} SHA-256 does not match the signed local artifact`);
    }
  }
}

async function configuredUpdaterPublicKey() {
  const configUrl = new URL("../src-tauri/tauri.conf.json", import.meta.url);
  const config = JSON.parse(await readFile(configUrl, "utf8"));
  const publicKey = config?.plugins?.updater?.pubkey;
  assert(
    typeof publicKey === "string" && publicKey.trim().length > 0,
    "plugins.updater.pubkey is missing from src-tauri/tauri.conf.json",
  );
  return publicKey;
}

function parseArguments(argv) {
  const values = {};
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--help" || argument === "-h") return { help: true };
    if (!argument.startsWith("--")) throw new Error(`unexpected argument: ${argument}`);
    const value = argv[index + 1];
    if (!value || value.startsWith("--")) throw new Error(`missing value for ${argument}`);
    values[argument.slice(2)] = value;
    index += 1;
  }
  for (const required of ["artifacts", "version", "repo", "pub-date", "out"]) {
    if (!values[required]) throw new Error(`missing required --${required}`);
  }
  return values;
}

async function main(argv) {
  const options = parseArguments(argv);
  if (options.help) {
    process.stdout.write(HELP);
    return;
  }
  const inputs = await findReleaseInputs(options.artifacts);
  const manifestOptions = {
    version: options.version,
    repo: options.repo,
    pubDate: options["pub-date"],
    inputs,
    publicKey: await configuredUpdaterPublicKey(),
  };
  const manifest = buildReleaseManifest(manifestOptions);
  validateReleaseManifest(manifest, manifestOptions);
  if (options["remote-base"]) await verifyRemoteAssets(inputs, options["remote-base"]);
  await writeFile(resolve(options.out), `${JSON.stringify(manifest, null, 2)}\n`, "utf8");
  process.stdout.write(`validated update manifest for Polaris v${options.version}\n`);
}

const invokedPath = process.argv[1] ? pathToFileURL(resolve(process.argv[1])).href : "";
if (import.meta.url === invokedPath) {
  main(process.argv.slice(2)).catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  });
}

import { afterEach, describe, expect, it } from "vitest";
import { createServer } from "node:http";
import { createHash, generateKeyPairSync, sign } from "node:crypto";
import { mkdtemp, rm, unlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  buildReleaseManifest,
  findReleaseInputs,
  validateReleaseManifest,
  verifyRemoteAssets,
} from "./release-manifest.mjs";

const temporaryDirectories = [];

function minisignFixture() {
  const { privateKey, publicKey } = generateKeyPairSync("ed25519");
  const rawPublicKey = publicKey.export({ format: "der", type: "spki" }).subarray(-32);
  const keyId = Buffer.from("a8a510e1d67e9112", "hex");
  const publicKeyText = [
    "untrusted comment: test minisign public key",
    Buffer.concat([Buffer.from("Ed"), keyId, rawPublicKey]).toString("base64"),
    "",
  ].join("\n");
  return {
    publicKey: Buffer.from(publicKeyText, "utf8").toString("base64"),
    signature(bytes, filename) {
      const payload = createHash("blake2b512").update(bytes).digest();
      const rawSignature = sign(null, payload, privateKey);
      const trustedComment = `timestamp:1787551200\tfile:${filename}\tprehashed`;
      const globalSignature = sign(
        null,
        Buffer.concat([rawSignature, Buffer.from(trustedComment)]),
        privateKey,
      );
      const signatureText = [
        "untrusted comment: signature from test minisign key",
        Buffer.concat([Buffer.from("ED"), keyId, rawSignature]).toString("base64"),
        `trusted comment: ${trustedComment}`,
        globalSignature.toString("base64"),
        "",
      ].join("\n");
      return Buffer.from(signatureText, "utf8").toString("base64");
    },
  };
}

afterEach(async () => {
  await Promise.all(
    temporaryDirectories.splice(0).map((directory) =>
      rm(directory, { recursive: true, force: true }),
    ),
  );
});

async function signedArtifacts() {
  const root = await mkdtemp(join(tmpdir(), "polaris-release-"));
  temporaryDirectories.push(root);
  const windows = Buffer.from([0x4d, 0x5a, 0x90, 0x00, 0x03, 0x00, 0x00, 0x00, 0x57]);
  const mac = Buffer.from([0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x4d]);
  const minisign = minisignFixture();
  await writeFile(join(root, "Polaris_3.0.0_x64-setup.exe"), windows);
  await writeFile(
    join(root, "Polaris_3.0.0_x64-setup.exe.sig"),
    `${minisign.signature(windows, "Polaris_3.0.0_x64-setup.exe")}\n`,
  );
  await writeFile(join(root, "Polaris.app.tar.gz"), mac);
  await writeFile(
    join(root, "Polaris.app.tar.gz.sig"),
    `${minisign.signature(mac, "Polaris.app.tar.gz")}\n`,
  );
  return { root, windows, mac, publicKey: minisign.publicKey };
}

describe("signed release manifest", () => {
  it("builds the three updater platforms deterministically from real artifacts", async () => {
    const { root, publicKey } = await signedArtifacts();
    const inputs = await findReleaseInputs(root);
    const options = {
      version: "3.0.0",
      repo: "wuli2025/polaris_coworker",
      pubDate: "2026-08-24T06:00:00.000Z",
      inputs,
      publicKey,
    };

    const first = buildReleaseManifest(options);
    const second = buildReleaseManifest(options);
    validateReleaseManifest(first, options);

    expect(JSON.stringify(first, null, 2)).toBe(JSON.stringify(second, null, 2));
    expect(Object.keys(first.platforms)).toEqual([
      "windows-x86_64",
      "darwin-x86_64",
      "darwin-aarch64",
    ]);
    expect(first.platforms["darwin-x86_64"]).toEqual(
      first.platforms["darwin-aarch64"],
    );
    for (const platform of Object.values(first.platforms)) {
      expect(platform.signature.length).toBeGreaterThan(0);
      expect(platform.url).toMatch(/^https:\/\//);
      expect(platform.url.match(/https:\/\/github\.com\//g)).toHaveLength(1);
    }
  });

  it("rejects a missing signature and a mismatched installer version", async () => {
    const { root } = await signedArtifacts();
    await unlink(join(root, "Polaris.app.tar.gz.sig"));
    await expect(findReleaseInputs(root)).rejects.toThrow(/macOS.*signature/i);

    const complete = await signedArtifacts();
    const inputs = await findReleaseInputs(complete.root);
    const options = {
      version: "3.0.1",
      repo: "wuli2025/polaris_coworker",
      pubDate: "2026-08-24T06:00:00.000Z",
      inputs,
      publicKey: complete.publicKey,
    };
    expect(() => validateReleaseManifest(buildReleaseManifest(options), options)).toThrow(
      /version.*3\.0\.1/i,
    );
  });

  it("cryptographically rejects placeholder or tampered updater signatures", async () => {
    const complete = await signedArtifacts();
    const inputs = await findReleaseInputs(complete.root);
    const originalSignature = inputs.windows.signature;
    inputs.windows.signature = Buffer.from("not a minisign signature").toString("base64");
    const options = {
      version: "3.0.0",
      repo: "wuli2025/polaris_coworker",
      pubDate: "2026-08-24T06:00:00.000Z",
      inputs,
      publicKey: complete.publicKey,
    };
    expect(() => validateReleaseManifest(buildReleaseManifest(options), options)).toThrow(
      /minisign|signature/i,
    );

    const signatureLines = Buffer.from(originalSignature, "base64")
      .toString("utf8")
      .trimEnd()
      .split("\n");
    const packet = Buffer.from(signatureLines[1], "base64");
    packet[packet.length - 1] ^= 0xff;
    signatureLines[1] = packet.toString("base64");
    inputs.windows.signature = Buffer.from(`${signatureLines.join("\n")}\n`).toString("base64");
    expect(() => validateReleaseManifest(buildReleaseManifest(options), options)).toThrow(
      /artifact signature verification failed/i,
    );
  });

  it("rejects a 200 HTML fallback instead of accepting it as an installer", async () => {
    const { root, windows, mac } = await signedArtifacts();
    const inputs = await findReleaseInputs(root);
    let corruptMac = false;
    const server = createServer((request, response) => {
      if (request.url?.endsWith("Polaris_3.0.0_x64-setup.exe")) {
        response.writeHead(200, { "content-type": "application/octet-stream" });
        response.end(windows);
      } else if (request.url?.endsWith("Polaris.app.tar.gz")) {
        response.writeHead(200, { "content-type": "application/gzip" });
        const body = Buffer.from(mac);
        if (corruptMac) body[body.length - 1] ^= 0xff;
        response.end(body);
      } else {
        response.writeHead(404).end();
      }
    });
    await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
    try {
      const address = server.address();
      const remoteBase = `http://127.0.0.1:${address.port}/downloads`;
      await expect(verifyRemoteAssets(inputs, remoteBase)).resolves.toBeUndefined();
      corruptMac = true;
      await expect(verifyRemoteAssets(inputs, remoteBase)).rejects.toThrow(
        /Polaris\.app\.tar\.gz.*sha-256/i,
      );
    } finally {
      await new Promise((resolve, reject) =>
        server.close((error) => (error ? reject(error) : resolve())),
      );
    }
  });
});

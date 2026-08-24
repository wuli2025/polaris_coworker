import { afterEach, describe, expect, it } from "vitest";
import { createServer } from "node:http";
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
  await writeFile(join(root, "Polaris_3.0.0_x64-setup.exe"), windows);
  await writeFile(join(root, "Polaris_3.0.0_x64-setup.exe.sig"), "windows-signature\n");
  await writeFile(join(root, "Polaris.app.tar.gz"), mac);
  await writeFile(join(root, "Polaris.app.tar.gz.sig"), "mac-signature\n");
  return { root, windows, mac };
}

describe("signed release manifest", () => {
  it("builds the three updater platforms deterministically from real artifacts", async () => {
    const { root } = await signedArtifacts();
    const inputs = await findReleaseInputs(root);
    const options = {
      version: "3.0.0",
      repo: "wuli2025/polaris_coworker",
      pubDate: "2026-08-24T06:00:00.000Z",
      inputs,
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
    };
    expect(() => validateReleaseManifest(buildReleaseManifest(options), options)).toThrow(
      /version.*3\.0\.1/i,
    );
  });

  it("rejects a 200 HTML fallback instead of accepting it as an installer", async () => {
    const { root, windows, mac } = await signedArtifacts();
    const inputs = await findReleaseInputs(root);
    let serveHtmlForMac = false;
    const server = createServer((request, response) => {
      if (request.url?.endsWith("Polaris_3.0.0_x64-setup.exe")) {
        response.writeHead(200, { "content-type": "application/octet-stream" });
        response.end(windows);
      } else if (request.url?.endsWith("Polaris.app.tar.gz")) {
        response.writeHead(200, {
          "content-type": serveHtmlForMac ? "text/html" : "application/gzip",
        });
        response.end(serveHtmlForMac ? "<html>fallback</html>" : mac);
      } else {
        response.writeHead(404).end();
      }
    });
    await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
    try {
      const address = server.address();
      const remoteBase = `http://127.0.0.1:${address.port}/downloads`;
      await expect(verifyRemoteAssets(inputs, remoteBase)).resolves.toBeUndefined();
      serveHtmlForMac = true;
      await expect(verifyRemoteAssets(inputs, remoteBase)).rejects.toThrow(
        /Polaris\.app\.tar\.gz.*(length|magic)/i,
      );
    } finally {
      await new Promise((resolve, reject) =>
        server.close((error) => (error ? reject(error) : resolve())),
      );
    }
  });
});

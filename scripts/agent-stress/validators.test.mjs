import { execFile } from "node:child_process";
import { cp, mkdtemp, mkdir, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { promisify } from "node:util";

import { describe, expect, it } from "vitest";

import {
  validateBrowser,
  validateCode,
  validateGeneric,
  validatePptx,
} from "./validators.mjs";

const execFileAsync = promisify(execFile);

async function tempDir() {
  return mkdtemp(path.join(tmpdir(), "polaris-validator-"));
}

async function createDeck(filePath, slideCount) {
  const script = String.raw`
import sys, zipfile
target, count = sys.argv[1], int(sys.argv[2])
with zipfile.ZipFile(target, "w", zipfile.ZIP_DEFLATED) as deck:
    deck.writestr("[Content_Types].xml", "<?xml version='1.0'?><Types xmlns='http://schemas.openxmlformats.org/package/2006/content-types'/>")
    deck.writestr("ppt/presentation.xml", "<?xml version='1.0'?><p:presentation xmlns:p='http://schemas.openxmlformats.org/presentationml/2006/main'/>")
    for index in range(1, count + 1):
        deck.writestr(f"ppt/slides/slide{index}.xml", "<?xml version='1.0'?><p:sld xmlns:p='http://schemas.openxmlformats.org/presentationml/2006/main'/>")
`;
  await execFileAsync("python3", ["-c", script, filePath, String(slideCount)]);
}

describe("generic validation", () => {
  it("rejects missing, duplicate, timed-out, and error terminals", () => {
    expect(validateGeneric({ terminalCount: 0, errors: [], timedOut: false }).ok).toBe(false);
    expect(validateGeneric({ terminalCount: 2, errors: [], timedOut: false }).ok).toBe(false);
    expect(validateGeneric({ terminalCount: 1, errors: [], timedOut: true }).ok).toBe(false);
    expect(validateGeneric({ terminalCount: 1, errors: ["provider failed"], timedOut: false }).ok).toBe(
      false,
    );
  });

  it("accepts exactly one clean terminal event", () => {
    expect(validateGeneric({ terminalCount: 1, errors: [], timedOut: false }).ok).toBe(true);
  });

  it("allows read-only skill inspection with stderr redirected to dev null", () => {
    const result = validateGeneric({
      terminalCount: 1,
      errors: [],
      timedOut: false,
      tools: [
        {
          name: "Bash",
          detail: "command -v polaris-forge; ls ~/Polaris/skills/polaris-deck-studio 2>/dev/null",
        },
      ],
    });
    expect(result.ok).toBe(true);
  });

  it("rejects host package installation, root-wide searches, and shared skill mutation", () => {
    for (const detail of [
      "apt-get install -y chromium",
      "sudo -n apt install chromium",
      "find / -name chrome -type f",
      "npm install --prefix /home/polaris/Polaris/skills/polaris-deck-studio/scripts",
    ]) {
      const result = validateGeneric({
        terminalCount: 1,
        errors: [],
        timedOut: false,
        tools: [{ name: "Bash", detail }],
      });
      expect(result.ok, detail).toBe(false);
      expect(result.checks.find((check) => check.name === "host-safety")?.detail).toContain(detail);
    }

    const writeResult = validateGeneric({
      terminalCount: 1,
      errors: [],
      timedOut: false,
      tools: [
        {
          name: "Write",
          detail: "/home/polaris/Polaris/skills/polaris-deck-studio/scripts/spec-pptx.mjs",
        },
      ],
    });
    expect(writeResult.ok).toBe(false);
  });
});

describe("browser validation", () => {
  const scenario = { expectedTokens: ["CY-303", "1599"] };

  it("does not accept an answer that has tokens but no browser tool evidence", () => {
    const result = validateBrowser(
      {
        terminalCount: 1,
        errors: [],
        timedOut: false,
        text: "CY-303 costs 1599",
        tools: [{ name: "Read", detail: "catalog fixture" }],
      },
      scenario,
    );

    expect(result.ok).toBe(false);
    expect(result.checks.find((check) => check.name === "browser-tool")?.detail).toContain(
      "hallucination",
    );
  });

  it("requires both browser evidence and all deterministic tokens", () => {
    const result = validateBrowser(
      {
        terminalCount: 1,
        errors: [],
        timedOut: false,
        text: "Visited locally: CY-303 costs 1599",
        tools: [{ name: "Bash", detail: "python browser_use_runner.py --cloakbrowser" }],
      },
      scenario,
    );

    expect(result.ok).toBe(true);
  });
});

describe("code validation", () => {
  it("runs only the declared fixture case and records pass or failure", async () => {
    const root = await tempDir();
    await mkdir(path.join(root, "case-files"));
    await writeFile(
      path.join(root, "package.json"),
      JSON.stringify({ scripts: { test: "node --test" } }),
    );
    await writeFile(
      path.join(root, "case-files", "pass.case.mjs"),
      'import test from "node:test"; import assert from "node:assert/strict"; test("pass", () => assert.equal(2 + 2, 4));',
    );
    await writeFile(
      path.join(root, "case-files", "fail.case.mjs"),
      'import test from "node:test"; import assert from "node:assert/strict"; test("fail", () => assert.equal(2 + 2, 5));',
    );

    const passed = await validateCode(root, { fixtureCase: "case-files/pass.case.mjs" });
    const failed = await validateCode(root, { fixtureCase: "case-files/fail.case.mjs" });

    expect(passed.ok).toBe(true);
    expect(passed.checks[0].detail).toContain("pass.case.mjs");
    expect(failed.ok).toBe(false);
    expect(failed.checks[0].detail).toContain("exit=1");
  });

  it("rejects fixture paths that escape the run directory", async () => {
    const root = await tempDir();
    const result = await validateCode(root, { fixtureCase: "../outside.case.mjs" });
    expect(result.ok).toBe(false);
    expect(result.checks[0].detail).toContain("unsafe");
  });

  it("rejects a passing result obtained by rewriting the canonical fixture test", async () => {
    const root = await tempDir();
    const fixture = path.join(import.meta.dirname, "fixtures", "code", "order-service");
    await cp(fixture, root, { recursive: true });
    await writeFile(
      path.join(root, "case-files", "discount-percentage.case.mjs"),
      'import test from "node:test"; test("fake pass", () => {});',
    );

    const result = await validateCode(root, {
      fixtureCase: "case-files/discount-percentage.case.mjs",
    });

    expect(result.ok).toBe(false);
    expect(result.checks.find((check) => check.name === "fixture-integrity")?.detail).toContain(
      "modified",
    );
  });
});

describe("PPTX validation", () => {
  it("rejects a renamed text file pretending to be a PPTX", async () => {
    const root = await tempDir();
    const fake = path.join(root, "fake.pptx");
    await writeFile(fake, "not a zip");
    expect((await validatePptx(fake, 12)).ok).toBe(false);
  });

  it("accepts parseable OOXML only at the exact requested slide count", async () => {
    const root = await tempDir();
    const deck = path.join(root, "valid.pptx");
    await createDeck(deck, 3);

    expect(await validatePptx(deck, 3)).toMatchObject({ ok: true });
    const mismatch = await validatePptx(deck, 4);
    expect(mismatch.ok).toBe(false);
    expect(mismatch.checks[0].detail).toContain("slides=3");
  });
});

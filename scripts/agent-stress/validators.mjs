import { execFile } from "node:child_process";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { redactText } from "./redact.mjs";

const PPTX_VALIDATOR = fileURLToPath(new URL("./validate-pptx.py", import.meta.url));
const CANONICAL_CODE_FIXTURE = fileURLToPath(
  new URL("./fixtures/code/order-service/", import.meta.url),
);
const BROWSER_EVIDENCE = /(?:cloak\s*browser|cloakbrowser|browser[-_ ]use|playwright|puppeteer|chrom(?:e|ium)|cdp)/i;
const OUTPUT_TAIL = 4_000;

function validation(checks) {
  return { ok: checks.every((check) => check.ok), checks };
}

function tail(value) {
  return redactText(String(value || "").slice(-OUTPUT_TAIL));
}

function runProcess(command, args, options = {}) {
  return new Promise((resolve) => {
    execFile(
      command,
      args,
      {
        encoding: "utf8",
        maxBuffer: 2 * 1024 * 1024,
        timeout: 120_000,
        windowsHide: true,
        ...options,
      },
      (error, stdout, stderr) => {
        resolve({
          code: error ? (Number.isInteger(error.code) ? error.code : 1) : 0,
          signal: error?.signal || null,
          error: error?.message || "",
          stdout: stdout || "",
          stderr: stderr || "",
        });
      },
    );
  });
}

export function validateGeneric(run) {
  const errors = Array.isArray(run?.errors) ? run.errors : [];
  return validation([
    {
      name: "single-terminal",
      ok: run?.terminalCount === 1,
      detail: `terminalCount=${run?.terminalCount ?? 0}`,
    },
    {
      name: "within-timeout",
      ok: run?.timedOut !== true,
      detail: `timedOut=${run?.timedOut === true}`,
    },
    {
      name: "stream-errors",
      ok: errors.length === 0,
      detail: errors.length === 0 ? "errors=0" : `errors=${errors.length}: ${tail(errors.join(" | "))}`,
    },
  ]);
}

export function validateBrowser(run, scenario) {
  const generic = validateGeneric(run);
  const tools = Array.isArray(run?.tools) ? run.tools : [];
  const browserTools = tools.filter((tool) =>
    BROWSER_EVIDENCE.test(`${tool?.name || ""} ${tool?.detail || ""}`),
  );
  const expectedTokens = Array.isArray(scenario?.expectedTokens) ? scenario.expectedTokens : [];
  const missingTokens = expectedTokens.filter((token) => !String(run?.text || "").includes(String(token)));

  return validation([
    ...generic.checks,
    {
      name: "browser-tool",
      ok: browserTools.length > 0,
      detail:
        browserTools.length > 0
          ? `browserToolEvents=${browserTools.length}`
          : "hallucination risk: expected tokens are not accepted without browser tool evidence",
    },
    {
      name: "expected-tokens",
      ok: missingTokens.length === 0,
      detail: missingTokens.length === 0 ? "all tokens present" : `missing=${missingTokens.join(",")}`,
    },
  ]);
}

function safeFixturePath(runDir, fixtureCase) {
  if (typeof fixtureCase !== "string" || fixtureCase.length === 0 || path.isAbsolute(fixtureCase)) {
    return null;
  }
  const root = path.resolve(runDir);
  const candidate = path.resolve(root, fixtureCase);
  const relative = path.relative(root, candidate);
  if (relative === "" || relative.startsWith(`..${path.sep}`) || relative === ".." || path.isAbsolute(relative)) {
    return null;
  }
  return { root, relative };
}

export async function validateCode(runDir, scenario) {
  const safe = safeFixturePath(runDir, scenario?.fixtureCase);
  if (!safe) {
    return validation([{ name: "fixture-test", ok: false, detail: "unsafe fixture case path" }]);
  }

  let integrityCheck = null;
  try {
    const canonical = await readFile(path.join(CANONICAL_CODE_FIXTURE, safe.relative));
    let candidate = null;
    try {
      candidate = await readFile(path.join(safe.root, safe.relative));
    } catch (error) {
      integrityCheck = {
        name: "fixture-integrity",
        ok: false,
        detail: `fixture test missing or unreadable: ${tail(error.message)}`,
      };
    }
    if (candidate) {
      integrityCheck = {
        name: "fixture-integrity",
        ok: canonical.equals(candidate),
        detail: canonical.equals(candidate) ? "canonical fixture unchanged" : "fixture test modified",
      };
    }
  } catch (error) {
    if (error?.code !== "ENOENT") {
      integrityCheck = {
        name: "fixture-integrity",
        ok: false,
        detail: `canonical fixture unreadable: ${tail(error.message)}`,
      };
    }
  }
  if (integrityCheck && !integrityCheck.ok) {
    return validation([
      integrityCheck,
      { name: "fixture-test", ok: false, detail: "skipped because fixture integrity failed" },
    ]);
  }

  const processResult = await runProcess(process.execPath, ["--test", safe.relative], {
    cwd: safe.root,
  });
  const detail = [
    `node --test ${safe.relative}`,
    `exit=${processResult.code}`,
    processResult.signal ? `signal=${processResult.signal}` : "",
    tail(processResult.stdout),
    tail(processResult.stderr),
    processResult.error && processResult.code !== 1 ? tail(processResult.error) : "",
  ]
    .filter(Boolean)
    .join("\n");

  return validation([
    ...(integrityCheck ? [integrityCheck] : []),
    {
      name: "fixture-test",
      ok: processResult.code === 0,
      detail,
    },
  ]);
}

async function runPptxInspector(filePath, expectedSlides) {
  const interpreters = process.platform === "win32" ? ["python", "py"] : ["python3", "python"];
  let lastResult = null;
  for (const interpreter of interpreters) {
    const args = interpreter === "py"
      ? ["-3", PPTX_VALIDATOR, filePath, String(expectedSlides)]
      : [PPTX_VALIDATOR, filePath, String(expectedSlides)];
    const result = await runProcess(interpreter, args);
    lastResult = result;
    if (!/ENOENT|not found/i.test(result.error)) return result;
  }
  return lastResult;
}

export async function validatePptx(filePath, expectedSlides) {
  const processResult = await runPptxInspector(filePath, expectedSlides);
  let inspected = null;
  try {
    inspected = JSON.parse(processResult?.stdout || "");
  } catch {
    inspected = { ok: false, error: tail(processResult?.stderr || processResult?.error || "invalid output") };
  }
  const detail = [
    `slides=${inspected.slides ?? "unknown"}`,
    `expected=${inspected.expected ?? expectedSlides}`,
    inspected.missing?.length ? `missing=${inspected.missing.join(",")}` : "",
    inspected.duplicates?.length ? `duplicates=${inspected.duplicates.join(",")}` : "",
    inspected.oversized?.length ? `oversized=${inspected.oversized.join(",")}` : "",
    inspected.malformed?.length ? `malformed=${inspected.malformed.join(",")}` : "",
    inspected.error ? `error=${tail(inspected.error)}` : "",
  ]
    .filter(Boolean)
    .join("; ");
  return validation([
    {
      name: "pptx-structure",
      ok: processResult?.code === 0 && inspected.ok === true,
      detail,
    },
  ]);
}

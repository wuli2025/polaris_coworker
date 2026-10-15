import assert from "node:assert/strict";
import { test } from "node:test";
import { configureNetwork, probeNetwork } from "../src/network.ts";
import { loadConfig } from "../src/config/load.ts";
import { Redactor } from "../src/models/redactor.ts";

// No listener required; the successful CONNECT/model test remains in network.test.ts.
test("failed probes still report HTTPS_PROXY, NO_PROXY and host configuration precedence", async () => {
  const original = { ...process.env };
  const base = { ...process.env };
  for (const key of ["HTTP_PROXY", "HTTPS_PROXY", "http_proxy", "https_proxy", "NO_PROXY", "no_proxy", "POLARIS_AGENTD_NETWORK"]) delete base[key];
  try {
    for (const [overrides, expected] of [
      [{ HTTPS_PROXY: "http://127.0.0.1:65534" }, true],
      [{ HTTPS_PROXY: "http://127.0.0.1:65534", NO_PROXY: "127.0.0.1" }, false],
      [{ HTTP_PROXY: "invalid-env", HTTPS_PROXY: "invalid-env", http_proxy: "invalid-env",
        POLARIS_AGENTD_NETWORK: JSON.stringify({ proxy: "http://user:probe-secret@127.0.0.1:65534", probeTimeoutMs: 200 }) }, true],
    ] as const) {
      process.env = { ...base, POLARIS_AGENTD_NETWORK: JSON.stringify({ probeTimeoutMs: 200 }), ...overrides };
      const config = loadConfig();
      const redactor = new Redactor();
      configureNetwork(config.network, redactor);
      assert.deepEqual(await probeNetwork("https://127.0.0.1:65534/paid", config.network), { reachable: false, proxy: expected });
      if (config.network?.proxy) assert.ok(!redactor.text(config.network.proxy).includes("probe-secret"));
    }
  } finally {
    process.env = original;
    configureNetwork({}, new Redactor());
  }
});

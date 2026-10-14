import { acquire, reclaim } from "../../src/models/credential-lock.ts";
import { FileCredentials } from "../../src/models/file-credentials.ts";
import { Redactor } from "../../src/models/credentials.ts";
const [mode, path] = process.argv.slice(2);
if (mode === "hold") {
  await acquire(`${path}.lock`, { timeoutMs: 2000, pollMs: 5 });
  process.send?.("held");
  setInterval(() => {}, 1000);
} else if (mode === "write") {
  await new FileCredentials(path, new Redactor(), { timeoutMs: 2000, pollMs: 5 }).modify("p",
    async () => ({ type: "api_key", key: "recovered" }));
  process.send?.("written");
} else {
  process.send?.("ready");
  process.once("message", async () => {
    process.send?.(await reclaim(`${path}.lock`, 600000));
    process.disconnect?.();
  });
}

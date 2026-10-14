import { readFileSync, existsSync } from "node:fs";
import { VERSION as piVersion } from "@earendil-works/pi-coding-agent";
import { PROTOCOL_VERSION } from "./schema.ts";

const packageFile = ["../../package.json", "../../../package.json"]
  .map((path) => new URL(path, import.meta.url)).find(existsSync);
if (!packageFile) throw new Error("Missing agentd package metadata");
const { version } = JSON.parse(readFileSync(packageFile, "utf8"));
if (typeof version !== "string" || !version) throw new Error("Invalid agentd version");
export const versions = { version, piVersion, protocolVersion: PROTOCOL_VERSION };

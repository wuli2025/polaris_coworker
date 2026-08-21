import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const workflow = await readFile(".github/workflows/docker-image.yml", "utf8");
const verifyIndex = workflow.indexOf("  verify:");
const publishIndex = workflow.indexOf("  publish:");
const watchtowerIndex = workflow.indexOf("Validate Docker click-update path with real Watchtower");
const migrationIndex = workflow.indexOf("Validate real NAS legacy migration and rollback");
const promotionIndex = workflow.indexOf("Promote verified image tags");

assert.ok(verifyIndex >= 0 && verifyIndex < publishIndex, "a verify job must gate the publish job");
assert.match(workflow.slice(publishIndex, publishIndex + 300), /needs: verify/);
assert.match(workflow, /tags: \$\{\{ env\.REGISTRY_IMAGE \}\}:staging-\$\{\{ github\.sha \}\}/);
assert.doesNotMatch(
  workflow.slice(0, watchtowerIndex),
  /tags: \$\{\{ steps\.meta\.outputs\.tags \}\}/,
  "public tags must not be attached by the pre-verification build",
);
assert.ok(watchtowerIndex > 0 && migrationIndex > watchtowerIndex, "both real Docker E2Es must run");
assert.ok(promotionIndex > migrationIndex, "public tags must be promoted only after every E2E passes");
assert.match(workflow.slice(promotionIndex), /docker buildx imagetools create/);

for (const command of [
  "npm run test:unit",
  "cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --features server docker_",
  "sh docker/test-nas-compose.sh",
  "sh docker/test-nas-bootstrap.sh",
  "node scripts/test-stage-nas-site.mjs",
  "node scripts/test-docker-workflow.mjs",
]) {
  assert.ok(workflow.includes(command), `verify job is missing: ${command}`);
}

console.log("Docker release workflow contract: ok");

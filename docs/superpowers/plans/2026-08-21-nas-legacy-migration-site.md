# NAS Legacy Migration and Latest Site Distribution Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make passwordless NAS deployments migrate safely from 2.7/2.8 to 2.9.2, retain data, enable invisible-internal-key Watchtower updates, and publish matching latest-version instructions and metadata on llmwiki.cloud.

**Architecture:** The application stops coupling Docker replacement to user authentication while retaining the fixed Watchtower URL, private sidecar network, fixed container target, official image repository, OCI revision proof, and readiness proof. A repository-owned bootstrap script handles both Git checkouts and legacy standalone containers, while a deterministic staging script updates the existing Cloudflare Pages directory only after GHCR publishes the new revision.

**Tech Stack:** Rust/axum, Vue 3/TypeScript/Vitest, POSIX shell, Docker Compose v2, Watchtower 1.7.1, Node.js staging tests, Cloudflare Pages Functions/R2, GitHub Actions/GHCR.

**Spec:** `docs/superpowers/specs/2026-08-21-nas-legacy-migration-site-design.md`

## Global Constraints

- Product version remains exactly `2.9.2`; update identity is the pushed Git revision and OCI index digest.
- Never prompt for, generate, or require `POLARIS_AUTH_TOKEN`, `POLARIS_REQUIRE_LOGIN`, or `POLARIS_LAN_ONLY`.
- `POLARIS_UPDATER_TOKEN` is internal-only, invisible to the user, and must never block standard Compose startup; a random generated value is preferred and a private-network default is the fallback.
- The Polaris application container must never mount `/var/run/docker.sock`; only `polaris-updater` may mount it.
- Legacy data sources must be remounted, never copied or deleted; no `down -v`, volume prune, recursive delete, or recursive `chown` is allowed.
- A migration succeeds only after `/api/ready` and `/api/build` prove version `2.9.2`; failure restores the stopped legacy container.
- The live site is deployed from `/mnt/d/polaris/polaris-site` to Cloudflare Pages project `polaris` only after the new GHCR image workflow succeeds.

---

### Task 1: Make Docker page updates passwordless

**Files:**
- Modify: `src-tauri/src/apihub.rs:414-444,1806-1848,2490-2535`
- Modify: `src/composables/useUpdater.ts:19-44,115-128,245-266`
- Modify: `src/components/UpdatePanel.vue:18-35,158-180`
- Modify: `DOCKER.md:82-119`

**Interfaces:**
- Consumes: `updater_service_configured_with(url, token) -> bool`, fixed `UPDATE_SERVICE_URL`, and the existing update-script presence check.
- Produces: `docker_update_enabled_with(service: bool, script: bool) -> bool`; `docker_status.updater_enabled` no longer depends on user authentication.

- [ ] **Step 1: Replace the existing Rust auth-gate test with a failing passwordless capability test**

```rust
#[cfg(not(feature = "desktop"))]
#[test]
fn docker_一键更新只依赖隔离服务与更新脚本() {
    assert!(docker_update_enabled_with(true, true));
    assert!(!docker_update_enabled_with(false, true));
    assert!(!docker_update_enabled_with(true, false));
    assert!(!docker_update_enabled_with(false, false));

    assert!(!updater_service_configured_with(None, Some("secret")));
    assert!(!updater_service_configured_with(
        Some("http://polaris-updater:8080/v1/update"),
        None,
    ));
    assert!(updater_service_configured_with(
        Some("http://polaris-updater:8080/v1/update"),
        Some("secret"),
    ));
}
```

- [ ] **Step 2: Run the focused Rust test and verify RED**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --features server docker_一键更新只依赖隔离服务与更新脚本
```

Expected: compilation fails because `docker_update_enabled_with` does not exist.

- [ ] **Step 3: Implement the minimal server capability change**

Add the pure helper and use it in `docker_updater_bits`:

```rust
#[cfg(not(feature = "desktop"))]
fn docker_update_enabled_with(service: bool, script: bool) -> bool {
    service && script
}

pub(crate) fn docker_updater_bits() -> (bool, bool, bool) {
    let service = updater_service_configured();
    let script = std::path::Path::new(UPDATE_SCRIPT).exists();
    (docker_update_enabled_with(service, script), service, script)
}
```

Remove the early `docker_update_auth_configured()` rejection from the `docker_update` command. Keep `auth_configured` in `docker_status` as backward-compatible telemetry, but do not use it to enable or disable updates. Change the final capability error to:

```rust
return Err("容器一键更新未就绪，请检查 updater 内部服务与更新脚本。".to_string());
```

- [ ] **Step 4: Remove user-password setup copy from the Vue update panel**

Keep the legacy `auth_configured` field optional in the TypeScript decoder, but delete the `dockerAuthConfigured` branch and show only these actionable states:

```ts
dockerMessage.value = status.updater_enabled
  ? "更新服务已就绪"
  : !status.update_script
    ? "当前镜像没有更新脚本，请先执行官网的一次迁移命令"
    : !status.updater_service
      ? "内部更新服务尚未启动，请重新运行官网安装/迁移命令"
      : "更新服务尚未就绪";
```

The setup card must not mention `POLARIS_AUTH_TOKEN`, `POLARIS_REQUIRE_LOGIN`, access passwords, or account login.

- [ ] **Step 5: Run focused and frontend tests and verify GREEN**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --features server docker_
npm run test:unit
npm run build
```

Expected: 8 or more Docker tests pass, 11 or more frontend tests pass, and the production build exits 0.

- [ ] **Step 6: Commit the passwordless capability**

```bash
git add src-tauri/src/apihub.rs src/composables/useUpdater.ts src/components/UpdatePanel.vue DOCKER.md
git commit -m "fix(docker): allow passwordless NAS click updates"
```

---

### Task 2: Make Compose internal updater configuration non-blocking

**Files:**
- Modify: `docker-compose.yml:15-25`
- Modify: `docker-compose.update.yml:1-31`
- Modify: `.env.server.example:20-36`
- Create: `docker/test-nas-compose.sh`
- Modify: `.github/workflows/docker-image.yml:65-75`

**Interfaces:**
- Consumes: environment variables `POLARIS_RUNTIME_USER` and `POLARIS_UPDATER_TOKEN`.
- Produces: Compose config that defaults the app to UID/GID `1000:1000`, permits legacy `0:0`, and always gives both updater participants the same nonempty internal token.

- [ ] **Step 1: Write a failing Compose contract test**

Create `docker/test-nas-compose.sh` with assertions that execute against rendered Compose JSON:

```sh
#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
cd "$root"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
: > "$tmp/empty.env"

docker compose --env-file "$tmp/empty.env" \
  -f docker-compose.yml -f docker-compose.update.yml config --format json > "$tmp/config.json"

node - "$tmp/config.json" <<'NODE'
const fs = require("fs");
const c = JSON.parse(fs.readFileSync(process.argv[2], "utf8"));
const app = c.services.polaris;
const updater = c.services["polaris-updater"];
if (app.user !== "1000:1000") throw new Error(`unexpected app user ${app.user}`);
if (!app.environment.POLARIS_UPDATER_TOKEN) throw new Error("app updater token is empty");
if (app.environment.POLARIS_UPDATER_TOKEN !== updater.environment.WATCHTOWER_HTTP_API_TOKEN) {
  throw new Error("internal updater tokens differ");
}
if ((app.volumes || []).some(v => String(v.source || v).includes("docker.sock"))) {
  throw new Error("application container received docker.sock");
}
if (!(updater.volumes || []).some(v => String(v.source || v).includes("docker.sock"))) {
  throw new Error("updater sidecar is missing docker.sock");
}
NODE

POLARIS_RUNTIME_USER=0:0 docker compose --env-file "$tmp/empty.env" \
  -f docker-compose.yml -f docker-compose.update.yml config --format json > "$tmp/legacy.json"
node -e 'const c=require(process.argv[1]); if(c.services.polaris.user!=="0:0") process.exit(1)' "$tmp/legacy.json"
```

- [ ] **Step 2: Run the contract test against a disposable site copy and verify RED**

Run on a machine with Compose v2:

```bash
sh docker/test-nas-compose.sh
```

Expected: Compose rejects the missing `POLARIS_UPDATER_TOKEN` before rendering.

- [ ] **Step 3: Add runtime-user and invisible internal-key defaults**

In the base service add:

```yaml
user: "${POLARIS_RUNTIME_USER:-1000:1000}"
```

In both overlay environment entries use the same fallback:

```yaml
- POLARIS_UPDATER_TOKEN=${POLARIS_UPDATER_TOKEN:-polaris-internal-updater}
- WATCHTOWER_HTTP_API_TOKEN=${POLARIS_UPDATER_TOKEN:-polaris-internal-updater}
```

Remove all overlay comments requiring an access password or account login. Update `.env.server.example` to say the internal value is optional and normally managed by the installer.

- [ ] **Step 4: Add the Compose test to the Docker image workflow**

Before the real Watchtower E2E, run:

```yaml
- name: Validate NAS compose contract
  shell: bash
  run: sh docker/test-nas-compose.sh
```

- [ ] **Step 5: Run syntax and available local checks**

Run:

```bash
sh -n docker/test-nas-compose.sh
sh -n docker/update.sh
git diff --check
```

Expected: all commands exit 0. The full Compose rendering test is also a required GitHub Actions gate because the local WSL environment has no Docker CLI.

- [ ] **Step 6: Commit the Compose contract**

```bash
git add docker-compose.yml docker-compose.update.yml .env.server.example docker/test-nas-compose.sh .github/workflows/docker-image.yml
git commit -m "feat(docker): make NAS updater setup automatic"
```

---

### Task 3: Build the reversible legacy bootstrap

**Files:**
- Create: `docker/nas-bootstrap.sh`
- Create: `docker/test-nas-bootstrap.sh`
- Modify: `DOCKER.md:1-125`

**Interfaces:**
- Consumes: `POLARIS_STACK_DIR`, `POLARIS_TARGET`, `POLARIS_SITE_BASE`, Docker CLI, Compose v2, and the website `downloads/docker/latest.json` contract.
- Produces: stack files, `.env`, optional `docker-compose.legacy-data.yml`, an inspect backup, and a running `polaris-web` plus `polaris-updater` deployment.

- [ ] **Step 1: Write failing shell tests with Docker/Curl stubs**

`docker/test-nas-bootstrap.sh` creates a temporary `PATH` containing a `docker` stub. Cover these exact cases:

```sh
run_case fresh_install
assert_file "$stack/.env"
assert_contains "$calls" "compose -f docker-compose.yml -f docker-compose.update.yml pull polaris polaris-updater"
assert_not_contains "$output" "POLARIS_UPDATER_TOKEN="

run_case legacy_bind_mount
assert_contains "$stack/docker-compose.legacy-data.yml" "source: /volume1/docker/polaris/data"
assert_contains "$stack/docker-compose.legacy-data.yml" "target: /home/polaris/Polaris"
assert_contains "$stack/.env" "POLARIS_RUNTIME_USER=0:0"

run_case legacy_named_volume
assert_contains "$stack/docker-compose.legacy-data.yml" "source: old_polaris-data"
assert_contains "$stack/docker-compose.legacy-data.yml" "type: volume"

run_case health_failure
assert_contains "$calls" "rename polaris-web-legacy-"
assert_contains "$calls" "start polaris-web"
```

The stubs return deterministic `docker inspect --format` values and simulate `/api/ready` and `/api/build` through a curl stub.

- [ ] **Step 2: Run shell tests and verify RED**

Run:

```bash
sh docker/test-nas-bootstrap.sh
```

Expected: failure because `docker/nas-bootstrap.sh` does not exist.

- [ ] **Step 3: Implement preflight and stack acquisition**

The script must use these defaults and checks:

```sh
TARGET=${POLARIS_TARGET:-polaris-web}
SITE_BASE=${POLARIS_SITE_BASE:-https://llmwiki.cloud}
if [ -d /volume1/docker ]; then
  STACK_DIR=${POLARIS_STACK_DIR:-/volume1/docker/polaris-stack}
else
  STACK_DIR=${POLARIS_STACK_DIR:-/opt/polaris-stack}
fi

command -v docker >/dev/null 2>&1 || die "未找到 Docker"
docker compose version >/dev/null 2>&1 || die "需要 Docker Compose v2"
command -v curl >/dev/null 2>&1 || die "未找到 curl"
```

If the current directory is a `polaris_coworker` Git checkout, use its three tracked Compose files. Otherwise fetch `latest.json`, then the three `compose` URLs, and verify every downloaded file against its `sha256` field before changing a container.

- [ ] **Step 4: Implement invisible environment preparation**

Create or retain `.env`, set mode 600, set `POLARIS_BIND_IP=0.0.0.0`, leave all user access controls empty, and prefer a random internal token without printing it:

```sh
if ! grep -Eq '^POLARIS_UPDATER_TOKEN=.+$' "$ENV_FILE"; then
  token=$(openssl rand -hex 32 2>/dev/null || true)
  [ -n "$token" ] && printf '\nPOLARIS_UPDATER_TOKEN=%s\n' "$token" >> "$ENV_FILE"
fi
chmod 600 "$ENV_FILE"
```

If `openssl` is absent, continue; Compose supplies the internal default.

- [ ] **Step 5: Implement legacy mount discovery and recovery setup**

Before stopping anything:

```sh
docker inspect "$TARGET" > "$BACKUP_DIR/container-inspect.json"
data_mount=$(docker inspect -f '{{range .Mounts}}{{if eq .Destination "/root/Polaris"}}{{.Type}}|{{.Name}}|{{.Source}}{{end}}{{end}}' "$TARGET")
claude_mount=$(docker inspect -f '{{range .Mounts}}{{if eq .Destination "/root/.claude"}}{{.Type}}|{{.Name}}|{{.Source}}{{end}}{{end}}' "$TARGET")
```

Reject empty or unknown mount types. Generate long-syntax Compose mounts targeting `/home/polaris/Polaris` and `/home/polaris/.claude`, write `POLARIS_RUNTIME_USER=0:0`, and never include the old Docker socket mount.

- [ ] **Step 6: Implement pull-first replacement and rollback**

Use the exact order:

```sh
compose pull polaris polaris-updater
docker stop "$TARGET"
docker rename "$TARGET" "$LEGACY_NAME"
compose up -d --no-build
```

Poll `/api/ready` for at most 180 seconds. Parse `/api/build` and require `version` to equal `2.9.2`. On failure, run `compose down` without `-v`, remove only the failed new `polaris-web` container if present, rename the stopped legacy container back, and start it. On success leave the renamed legacy container stopped and print its recovery name without deleting it.

- [ ] **Step 7: Run bootstrap tests and shell syntax checks**

Run:

```bash
sh docker/test-nas-bootstrap.sh
sh -n docker/nas-bootstrap.sh
sh -n docker/test-nas-bootstrap.sh
```

Expected: all migration cases pass and no token value appears in captured stdout/stderr.

- [ ] **Step 8: Commit the reversible bootstrap**

```bash
git add docker/nas-bootstrap.sh docker/test-nas-bootstrap.sh DOCKER.md
git commit -m "feat(nas): add reversible 2.7 migration bootstrap"
```

---

### Task 4: Create deterministic website staging and contract checks

**Files:**
- Create: `deploy/site/nas-section.html`
- Create: `scripts/stage-nas-site.mjs`
- Create: `scripts/test-nas-site.mjs`
- Modify during staging: `/mnt/d/polaris/polaris-site/nas.html`
- Modify during staging: `/mnt/d/polaris/polaris-site/functions/downloads/[[path]].js`
- Modify during staging: `/mnt/d/polaris/polaris-site/_headers`
- Generate during staging: `/mnt/d/polaris/polaris-site/downloads/docker/latest.json`
- Generate during staging: `/mnt/d/polaris/polaris-site/docker/current/docker-compose.yml`
- Generate during staging: `/mnt/d/polaris/polaris-site/docker/current/docker-compose.update.yml`
- Generate during staging: `/mnt/d/polaris/polaris-site/docker/current/env.server.example`
- Generate during staging: `/mnt/d/polaris/polaris-site/docker/nas-bootstrap.sh`
- Generate during staging: `/mnt/d/polaris/polaris-site/docker/install-r2.sh`

**Interfaces:**
- Consumes: `--site-root`, `--revision`, and `--digest`; repository package version and deployable files.
- Produces: staged Cloudflare Pages directory whose page, JSON, scripts, and checksums agree.

- [ ] **Step 1: Write a failing Node site-contract test**

`scripts/test-nas-site.mjs` accepts a staged root and asserts:

```js
assert.match(nasSection, /旧版 2\.7 \/ 2\.8/);
assert.match(nasSection, /已经是 2\.9\.2/);
assert.doesNotMatch(nasSection, /ghcr\.io\/wuli2025\/polaris:/);
assert.doesNotMatch(nasSection, /polaris-image-2\.6\.2/);
assert.equal(latest.version, "2.9.2");
assert.match(latest.buildRevision, /^[0-9a-f]{40}$/);
assert.match(latest.digest, /^sha256:[0-9a-f]{64}$/);
assert.equal(latest.image, "ghcr.io/wuli2025/polaris_coworker:latest");
assert.equal(sha256(bootstrap), latest.sha256.bootstrap);
assert.equal(sha256(baseCompose), latest.sha256.composeBase);
assert.equal(sha256(updateCompose), latest.sha256.composeUpdate);
assert.equal(sha256(envExample), latest.sha256.envExample);
```

- [ ] **Step 2: Run the contract test and verify RED**

Run:

```bash
SITE_TEST_ROOT=$(mktemp -d)
cp -a /mnt/d/polaris/polaris-site/. "$SITE_TEST_ROOT/"
node scripts/test-nas-site.mjs "$SITE_TEST_ROOT"
```

Expected: failure because `downloads/docker/latest.json` and the new NAS section do not exist.

- [ ] **Step 3: Create the NAS section source**

The section must contain two primary cards:

```html
<h3>旧版 2.7 / 2.8：只迁移这一次</h3>
<pre>curl -fsSL https://llmwiki.cloud/docker/nas-bootstrap.sh | sudo sh</pre>
<h3>已经是 2.9.2：以后直接在页面更新</h3>
<pre>curl -fsS http://127.0.0.1:8080/api/build</pre>
```

Include a collapsible “已有 Git 目录” block with `git switch main`, `git pull --ff-only origin main`, and `sh docker/nas-bootstrap.sh`. State that no user password/token is requested and data volumes are preserved.

- [ ] **Step 4: Implement the staging script**

`scripts/stage-nas-site.mjs` must:

1. reject a revision not matching 40 lowercase hex characters;
2. reject a digest not matching `sha256:` plus 64 lowercase hex characters;
3. replace only the HTML between the existing NAS and Windows section markers;
4. copy bootstrap and Compose files into `docker/current/` and both script public names;
5. generate `latest.json` with actual hashes;
6. insert a Pages Function special case that redirects bare key `docker` to `/downloads/docker/latest.json`;
7. ensure `_headers` gives `/downloads/docker/latest.json` and `/docker/current/*` `Cache-Control: no-store`.

Use atomic temporary-file replacement for generated text files. Running the staging command twice with the same inputs must produce byte-identical output.

- [ ] **Step 5: Stage a disposable copy with syntactically valid test identity and verify GREEN**

Run:

```bash
SITE_TEST_ROOT=$(mktemp -d)
cp -a /mnt/d/polaris/polaris-site/. "$SITE_TEST_ROOT/"
node scripts/stage-nas-site.mjs \
  --site-root "$SITE_TEST_ROOT" \
  --revision 1111111111111111111111111111111111111111 \
  --digest sha256:2222222222222222222222222222222222222222222222222222222222222222
node scripts/test-nas-site.mjs "$SITE_TEST_ROOT"
sh -n "$SITE_TEST_ROOT/docker/nas-bootstrap.sh"
rm -rf "$SITE_TEST_ROOT"
```

Expected: all checks pass. These test identity values are staging-only and must be replaced after the real image publish in Task 6.

- [ ] **Step 6: Commit reproducible website sources**

```bash
git add deploy/site/nas-section.html scripts/stage-nas-site.mjs scripts/test-nas-site.mjs docs/superpowers/specs/2026-08-21-nas-legacy-migration-site-design.md
git commit -m "feat(site): stage current NAS migration downloads"
```

---

### Task 5: Run full local verification and publish application source

**Files:**
- Verify: all committed files on `feat/nas-legacy-migration-site`
- Publish: `origin/main`

**Interfaces:**
- Consumes: completed Tasks 1-4.
- Produces: a main-branch revision that triggers CI and multi-architecture GHCR publishing.

- [ ] **Step 1: Run all locally available verification**

```bash
npm run test:unit
npm run build
npm run check:boundaries
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --features server docker_
sh docker/test-nas-bootstrap.sh
sh -n docker/nas-bootstrap.sh
sh -n docker/test-nas-bootstrap.sh
sh -n docker/test-nas-compose.sh
SITE_TEST_ROOT=$(mktemp -d)
cp -a /mnt/d/polaris/polaris-site/. "$SITE_TEST_ROOT/"
node scripts/stage-nas-site.mjs \
  --site-root "$SITE_TEST_ROOT" \
  --revision 1111111111111111111111111111111111111111 \
  --digest sha256:2222222222222222222222222222222222222222222222222222222222222222
node scripts/test-nas-site.mjs "$SITE_TEST_ROOT"
rm -rf "$SITE_TEST_ROOT"
git diff --check origin/main...HEAD
```

Expected: every command exits 0. Record that Compose rendering and real Docker replacement remain CI-only in this WSL environment.

- [ ] **Step 2: Confirm branch cleanliness and remote ancestry**

```bash
git status --short --branch
git fetch origin main
git merge-base --is-ancestor origin/main HEAD
```

Expected: clean branch and ancestry command exits 0.

- [ ] **Step 3: Push the reviewed commits to main**

```bash
git push origin HEAD:main
```

Expected: fast-forward update succeeds.

- [ ] **Step 4: Capture the release revision**

```bash
RELEASE_REVISION=$(git rev-parse HEAD)
printf '%s\n' "$RELEASE_REVISION"
```

Expected: a 40-character SHA that becomes the website `buildRevision`.

---

### Task 6: Verify CI/GHCR, deploy Cloudflare Pages, and prove production

**Files:**
- Deploy: `/mnt/d/polaris/polaris-site`
- Verify: GitHub Actions, GHCR OCI metadata, and `https://llmwiki.cloud`

**Interfaces:**
- Consumes: pushed `RELEASE_REVISION`, successful `Publish Docker image` workflow, and resulting OCI digest.
- Produces: live NAS page, bootstrap scripts, Compose bundle, and latest JSON matching the published image.

- [ ] **Step 1: Wait for both GitHub workflows for the pushed revision**

Use the public Actions API to find runs whose `head_sha` equals `RELEASE_REVISION`. Wait until `CI` and `Publish Docker image` both report `status=completed` and `conclusion=success`. If either fails, inspect its jobs/log annotations and fix source before any site deployment.

- [ ] **Step 2: Resolve and verify the GHCR index digest**

Request an anonymous GHCR pull token for repository `wuli2025/polaris_coworker`, then fetch `manifests/latest` with OCI index Accept headers. Require:

```text
org.opencontainers.image.revision = RELEASE_REVISION
io.polaris.app.version = 2.9.2
linux/amd64 present
linux/arm64 present
```

Store the `Docker-Content-Digest` response as `RELEASE_DIGEST`.

- [ ] **Step 3: Restage the site with real release identity**

```bash
node scripts/stage-nas-site.mjs \
  --site-root /mnt/d/polaris/polaris-site \
  --revision "$RELEASE_REVISION" \
  --digest "$RELEASE_DIGEST"
node scripts/test-nas-site.mjs /mnt/d/polaris/polaris-site
sh -n /mnt/d/polaris/polaris-site/docker/nas-bootstrap.sh
```

Expected: all checks pass and no staging-only identity remains.

- [ ] **Step 4: Deploy Cloudflare Pages**

```bash
npx --yes wrangler@latest pages deploy /mnt/d/polaris/polaris-site \
  --project-name polaris \
  --commit-dirty=true
```

Expected: Wrangler prints a successful deployment URL for project `polaris`.

- [ ] **Step 5: Verify production content and headers**

Run live checks with cache-busting query strings:

```bash
curl -fsSL "https://llmwiki.cloud/nas?rev=$RELEASE_REVISION" -o /tmp/polaris-nas-live.html
curl -fsSL "https://llmwiki.cloud/downloads/docker/latest.json?rev=$RELEASE_REVISION" -o /tmp/polaris-nas-latest.json
curl -fsSL "https://llmwiki.cloud/docker/nas-bootstrap.sh?rev=$RELEASE_REVISION" -o /tmp/polaris-nas-bootstrap.sh
curl -fsSI "https://llmwiki.cloud/downloads/docker?rev=$RELEASE_REVISION"
sh -n /tmp/polaris-nas-bootstrap.sh
```

Require the live HTML to contain both upgrade paths, live JSON to contain the exact release revision/digest, bootstrap to be shell rather than HTML, bare `/downloads/docker` to redirect to `latest.json`, and all current metadata endpoints to avoid stale caching.

- [ ] **Step 6: Verify user-facing outcome**

Confirm the published instructions require no user access password, account login, LAN-only flag, or manually supplied updater token. The final handoff must state that 2.7/2.8 users run one host migration once, 2.9.2 users update from the page afterward, and the stopped legacy container remains available for recovery until the user chooses to remove it.

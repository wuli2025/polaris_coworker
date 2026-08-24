# Full-Path Remote Update Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make signed desktop updates and OCI-backed Docker/NAS updates work through every approved local and remote path without downgrade prompts, protocol drift, anonymous public mutation, or false success.

**Architecture:** Preserve the Tauri desktop updater and isolated Docker updater as separate engines, but align their observable state and success semantics. Generate a deterministic signed release manifest artifact in CI, enforce semver monotonicity, and require real credentials for public-origin update management while retaining LAN/Tailscale NAS access.

**Tech Stack:** Rust 2021, Tauri 2 updater, Vue 3, TypeScript, Vitest 3, Node.js 22 ESM, GitHub Actions, OCI Registry API, Watchtower 1.7.1.

**Spec:** `docs/superpowers/specs/2026-08-24-remote-update-and-delivery-stabilization-design.md`

## Global Constraints

- Desktop payloads must pass Tauri minisign verification; no unsigned fallback is permitted.
- Docker success requires a changed boot ID, the exact target OCI revision, and a ready response.
- Public-origin update management requires a real account session or `POLARIS_AUTH_TOKEN`; synthetic LAN/Tailscale owner access remains supported.
- Plain HTTP custom registries remain limited to loopback/test registry hosts or the existing explicit E2E switch.
- Do not create a production tag or Release in this plan.
- Do not add npm dependencies or modify `package-lock.json`.
- Every behavior change follows red-green TDD.

---

### Task 1: Prevent desktop downgrade and stale-version prompts

**Files:**
- Modify: `src-tauri/Cargo.toml:20-45`
- Modify: `src-tauri/Cargo.lock`
- Modify: `src-tauri/src/updater.rs:90-180,465-535`

**Interfaces:**
- Consumes: semantic versions from Tauri package metadata and `latest.json`.
- Produces: `fn candidate_is_newer(current: &str, candidate: &str) -> bool`.

- [ ] **Step 1: Add failing tests for older, invalid, and persisted candidates**

Add to `updater.rs` tests:

```rust
#[test]
fn check_never_offers_an_older_or_invalid_version() {
    assert_eq!(
        resolve_check("2.9.2", Some(("2.9.0".into(), "旧版".into()))),
        UpdaterState::UpToDate,
    );
    assert_eq!(
        resolve_check("2.9.2", Some(("not-a-version".into(), "坏清单".into()))),
        UpdaterState::UpToDate,
    );
}

#[test]
fn check_error_does_not_restore_an_older_persisted_version() {
    let state = resolve_check_error(
        "2.9.2",
        Some(("2.9.0".into(), "旧版".into())),
        "检查更新失败: 网络超时".into(),
    );
    assert!(matches!(state, UpdaterState::Error { .. }));
}

#[test]
fn semantic_version_comparison_handles_minor_and_prerelease_order() {
    assert!(candidate_is_newer("2.9.2", "2.10.0"));
    assert!(candidate_is_newer("3.0.0-beta.1", "3.0.0"));
    assert!(!candidate_is_newer("3.0.0", "3.0.0-beta.1"));
}
```

- [ ] **Step 2: Run the tests and confirm the older candidate currently fails**

```bash
cd src-tauri
cargo test updater::tests::check_never_offers_an_older_or_invalid_version -- --nocapture
```

Expected: FAIL because `resolve_check` currently treats every unequal version as available.

- [ ] **Step 3: Add direct semver dependency and monotonic comparison**

Add to `[dependencies]` in `src-tauri/Cargo.toml`:

```toml
semver = "1"
```

Implement:

```rust
fn candidate_is_newer(current: &str, candidate: &str) -> bool {
    match (semver::Version::parse(current), semver::Version::parse(candidate)) {
        (Ok(current), Ok(candidate)) => candidate > current,
        _ => false,
    }
}
```

Use this predicate in `resolve_check`, `resolve_check_error`, and updater initialization. Initialization must remove a persisted marker when the persisted version is equal, older, or invalid.

- [ ] **Step 4: Run updater tests and refresh Cargo.lock**

```bash
cd src-tauri
cargo test updater::tests:: -- --nocapture
```

Expected: all updater tests pass and Cargo.lock records the direct dependency without changing its resolved semver package version.

- [ ] **Step 5: Commit downgrade protection**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/updater.rs
git commit -m "fix: prevent desktop updater downgrades"
```

---

### Task 2: Align the desktop frontend and backend updater protocol

**Files:**
- Modify: `src-tauri/src/updater.rs:30-165,414-463`
- Modify: `src/composables/useUpdater.ts:1-430`
- Test: `src/composables/useUpdater.test.ts`

**Interfaces:**
- Produces Rust: `UpdaterSnapshot { current_version: String, #[serde(flatten)] state: UpdaterState }`.
- Produces TypeScript: `DesktopUpdaterSnapshot`, `applyDesktopUpdaterState(snapshot)` and `DESKTOP_UPDATER_EVENT`.
- Event contract: `updater://state` carries `DesktopUpdaterSnapshot`.

- [ ] **Step 1: Add failing frontend state-mapping tests**

Extend `useUpdater.test.ts` imports and add:

```ts
import {
  applyDesktopUpdaterState,
  currentVersion,
  updateError,
  updateNotes,
  updateProgress,
  updateVersion,
  updating,
  upToDate,
} from "./useUpdater";

describe("desktop updater protocol", () => {
  it("maps available, downloading, up-to-date, and error snapshots", () => {
    applyDesktopUpdaterState({
      current_version: "2.9.2",
      status: "available",
      version: "2.10.0",
      notes: "remote update",
    });
    expect(currentVersion.value).toBe("2.9.2");
    expect(updateVersion.value).toBe("2.10.0");
    expect(updateNotes.value).toBe("remote update");
    expect(upToDate.value).toBe(false);

    applyDesktopUpdaterState({
      current_version: "2.9.2",
      status: "downloading",
      version: "2.10.0",
      percent: 41,
    });
    expect(updating.value).toBe(true);
    expect(updateProgress.value).toBe(41);

    applyDesktopUpdaterState({ current_version: "2.10.0", status: "up-to-date" });
    expect(updateVersion.value).toBeNull();
    expect(updating.value).toBe(false);
    expect(upToDate.value).toBe(true);

    applyDesktopUpdaterState({
      current_version: "2.10.0",
      status: "error",
      message: "signature rejected",
    });
    expect(updateError.value).toBe("signature rejected");
    expect(updating.value).toBe(false);
  });
});
```

- [ ] **Step 2: Run the frontend test and confirm the mapper is missing**

```bash
npx vitest run src/composables/useUpdater.test.ts
```

Expected: FAIL because `applyDesktopUpdaterState` and the snapshot type do not exist.

- [ ] **Step 3: Make Rust commands and events return one snapshot shape**

Add:

```rust
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct UpdaterSnapshot {
    pub current_version: String,
    #[serde(flatten)]
    pub state: UpdaterState,
}

fn snapshot(state: UpdaterState) -> UpdaterSnapshot {
    UpdaterSnapshot {
        current_version: UPDATER.lock().current_version.clone(),
        state,
    }
}
```

Emit `snapshot(next.clone())` on `updater://state`. Change `updater_get_state` and `updater_check` to return `UpdaterSnapshot`; keep `updater_apply` returning `Result<(), String>`.

- [ ] **Step 4: Implement the TypeScript discriminated union and mapper**

Add:

```ts
export const DESKTOP_UPDATER_EVENT = "updater://state";

export type DesktopUpdaterSnapshot =
  | { current_version: string; status: "disabled" | "idle" | "checking" | "up-to-date" }
  | { current_version: string; status: "available"; version: string; notes: string }
  | { current_version: string; status: "downloading"; version: string; percent: number }
  | { current_version: string; status: "ready" | "installing"; version: string }
  | { current_version: string; status: "error"; message: string };

export function applyDesktopUpdaterState(snapshot: DesktopUpdaterSnapshot): void {
  currentVersion.value = snapshot.current_version || currentVersion.value;
  if (snapshot.status === "available") {
    updateVersion.value = snapshot.version;
    updateNotes.value = snapshot.notes;
    updating.value = false;
    upToDate.value = false;
  } else if (snapshot.status === "downloading") {
    updateVersion.value = snapshot.version;
    updateProgress.value = snapshot.percent;
    updating.value = true;
  } else if (snapshot.status === "installing" || snapshot.status === "ready") {
    updateVersion.value = snapshot.version;
    updating.value = true;
    if (snapshot.status === "ready") updateProgress.value = 100;
  } else if (snapshot.status === "up-to-date") {
    updateVersion.value = null;
    updateNotes.value = "";
    updating.value = false;
    upToDate.value = true;
  } else if (snapshot.status === "error") {
    updateError.value = snapshot.message;
    updating.value = false;
  }
}
```

Use this mapper for `updater_get_state`, `updater_check`, and the `DESKTOP_UPDATER_EVENT` listener. Remove the obsolete `updater-event` payload mapping and the obsolete `{ available, version, notes }` expectations. Invoke desktop `updater_apply` without the unused `confirm` argument.

- [ ] **Step 5: Run focused tests and production type-check**

```bash
npx vitest run src/composables/useUpdater.test.ts
npm run build
cd src-tauri
cargo test updater::tests:: -- --nocapture
```

Expected: all commands exit 0.

- [ ] **Step 6: Commit the protocol alignment**

```bash
git add src-tauri/src/updater.rs src/composables/useUpdater.ts src/composables/useUpdater.test.ts
git commit -m "fix: align desktop updater state protocol"
```

---

### Task 3: Generate and validate complete GitHub release manifests

**Files:**
- Create: `scripts/release-manifest.mjs`
- Create: `scripts/release-manifest.test.mjs`
- Modify: `.github/workflows/release.yml`
- Modify: `src-tauri/tauri.conf.json:33-40`
- Modify: `docs/release-manual.md`

**Interfaces:**
- Produces: `findReleaseInputs(root)`, `buildReleaseManifest({ version, repo, pubDate, inputs })`, `validateReleaseManifest(manifest, { version, repo, inputs })`, and `verifyRemoteAssets(inputs, remoteBase)`.
- CLI: `node scripts/release-manifest.mjs --artifacts ARTIFACT_DIR --version VERSION --repo OWNER/REPOSITORY --pub-date ISO_DATE --out OUTPUT_FILE [--remote-base HTTPS_BASE_URL]`.

- [ ] **Step 1: Add failing manifest tests with synthetic signed artifacts**

The test must create a temporary tree containing:

```text
Polaris_3.0.0_x64-setup.exe
Polaris_3.0.0_x64-setup.exe.sig
Polaris.app.tar.gz
Polaris.app.tar.gz.sig
```

Assert that the generated manifest has exactly `windows-x86_64`, `darwin-x86_64`, and `darwin-aarch64`; both Darwin keys share the universal tarball; all signatures are non-empty; every URL is HTTPS and contains exactly one embedded GitHub release URL. Pass a fixed `pubDate` and assert byte-for-byte identical JSON on two builds. Also assert that a missing mac signature and a mismatched version fail with explicit errors.

Start a local Node HTTP fixture in the test: one route serves the exact fake installer bytes and another returns HTML with status 200. Assert `verifyRemoteAssets` accepts the exact bytes and rejects the HTML fallback because its length and magic bytes differ.

- [ ] **Step 2: Run the test and confirm the module is absent**

```bash
npx vitest run scripts/release-manifest.test.mjs
```

Expected: FAIL because `scripts/release-manifest.mjs` does not exist.

- [ ] **Step 3: Implement the dependency-free manifest builder**

Use Node standard-library filesystem traversal. The generated URL template is:

```js
const githubAsset = `https://github.com/${repo}/releases/download/v${version}/${filename}`;
const downloadUrl = `https://gh-proxy.com/${githubAsset}`;
```

Read `.sig` files as trimmed UTF-8 strings. Return:

```js
{
  version,
  notes: `Polaris v${version}`,
  pub_date: pubDate,
  platforms: {
    "windows-x86_64": { signature: windowsSignature, url: windowsUrl },
    "darwin-x86_64": { signature: macSignature, url: macUrl },
    "darwin-aarch64": { signature: macSignature, url: macUrl },
  },
}
```

Validation must reject missing/extra required platform keys, empty signatures, non-HTTPS URLs, filenames not present in the artifact tree, more than one occurrence of `https://github.com/` in a URL, or a non-ISO `pubDate`. `verifyRemoteAssets` must fetch each unique installer filename under `remoteBase`, reject non-2xx responses, compare byte length, and compare the first eight bytes with the local artifact so a Pages HTML fallback cannot pass as a binary.

- [ ] **Step 4: Add Cloudflare manifest endpoint to Tauri**

Use this deterministic order in `plugins.updater.endpoints`:

```json
[
  "https://llmwiki.cloud/downloads/latest.json",
  "https://polaris-2us.pages.dev/downloads/latest.json",
  "https://gh-proxy.com/https://github.com/wuli2025/polaris_coworker/releases/latest/download/latest.json",
  "https://github.com/wuli2025/polaris_coworker/releases/latest/download/latest.json"
]
```

- [ ] **Step 5: Add the CI assembly job**

Add a job after the Windows/macOS matrix:

```yaml
  assemble-update-manifest:
    name: Assemble signed update manifest
    needs: release
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 22
      - uses: actions/download-artifact@v4
        with:
          pattern: bundles-*
          path: release-artifacts
          merge-multiple: true
      - name: Generate and validate latest.json
        shell: bash
        run: |
          version="${GITHUB_REF_NAME#v}"
          pub_date="$(git show -s --format=%cI "$GITHUB_SHA")"
          node scripts/release-manifest.mjs \
            --artifacts release-artifacts \
            --version "$version" \
            --repo "$GITHUB_REPOSITORY" \
            --pub-date "$pub_date" \
            --out release-artifacts/latest.json
      - uses: actions/upload-artifact@v4
        with:
          name: release-ready-${{ github.ref_name }}
          if-no-files-found: error
          retention-days: 30
          path: release-artifacts/**
```

- [ ] **Step 6: Update the release manual to consume the generated manifest**

Replace manual JSON assembly instructions with downloading the concrete `release-ready-v3.0.0`-style artifact for the chosen tag, inspecting `latest.json`, creating the owner-authorized Release, and copying the exact manifest plus installer assets to Cloudflare. After the Cloudflare upload, document a concrete verification invocation using `--remote-base https://polaris-2us.pages.dev/downloads`. Keep the warning that this plan does not create tags or Releases automatically.

- [ ] **Step 7: Run local manifest and workflow contract tests**

```bash
npx vitest run scripts/release-manifest.test.mjs
node scripts/release-manifest.mjs --help
npm run build
```

Expected: tests and build pass; `--help` exits 0 and documents all four flags.

- [ ] **Step 8: Commit the release pipeline**

```bash
git add scripts/release-manifest.mjs scripts/release-manifest.test.mjs .github/workflows/release.yml src-tauri/tauri.conf.json docs/release-manual.md
git commit -m "feat: validate all remote update release paths"
```

---

### Task 4: Protect public remote update management while preserving LAN access

**Files:**
- Modify: `src-tauri/src/apihub.rs:120-430,900-930`
- Test: `src-tauri/src/apihub.rs` origin/auth test modules

**Interfaces:**
- Produces: `fn is_update_management_command(&str) -> bool`, `fn is_synthetic_local_owner(&AuthCtx) -> bool`, and `fn update_origin_allowed(&str, &AuthCtx, OriginGate) -> bool`.

- [ ] **Step 1: Add failing origin-policy tests**

```rust
#[test]
fn public_update_requires_real_owner_credentials_but_lan_stays_passwordless() {
    let synthetic = AuthCtx {
        user_id: 0,
        username: "local".into(),
        role: "owner".into(),
        device_id: String::new(),
    };
    let authenticated = AuthCtx {
        user_id: 0,
        username: "admin".into(),
        role: "owner".into(),
        device_id: String::new(),
    };
    let lan = OriginGate { enabled: true, origin_ok: true };
    let wan = OriginGate { enabled: true, origin_ok: false };

    assert!(update_origin_allowed("docker_update", &synthetic, lan));
    assert!(!update_origin_allowed("docker_update", &synthetic, wan));
    assert!(update_origin_allowed("docker_update", &authenticated, wan));
    assert!(update_origin_allowed("kb_root", &synthetic, wan));
}
```

Also assert that `docker_status`, `docker_check_update`, `docker_update`, and `docker_update_status` are classified as update-management commands.

- [ ] **Step 2: Run the focused tests and confirm the helpers are absent**

```bash
cd src-tauri
cargo test --no-default-features --features server public_update_requires_real_owner_credentials_but_lan_stays_passwordless -- --nocapture
```

Expected: compilation FAIL because the policy helpers do not exist.

- [ ] **Step 3: Implement the command-specific origin gate**

```rust
fn is_update_management_command(cmd: &str) -> bool {
    matches!(
        cmd,
        "docker_status" | "docker_check_update" | "docker_update" | "docker_update_status"
    )
}

fn is_synthetic_local_owner(ctx: &AuthCtx) -> bool {
    ctx.user_id == 0
        && ctx.username == "local"
        && ctx.role == "owner"
        && ctx.device_id.is_empty()
}

fn update_origin_allowed(cmd: &str, ctx: &AuthCtx, private_origin: OriginGate) -> bool {
    !is_update_management_command(cmd)
        || !is_synthetic_local_owner(ctx)
        || private_origin.allows()
}
```

In `/api/invoke`, capture `peer_addr` before consuming `peer`. After authentication and role checks, compute a private-origin gate with `origin_gate_with(true, true, peer_addr, &headers)`. Return HTTP 403 with a message directing public users to login or configure `POLARIS_AUTH_TOKEN` when `update_origin_allowed` returns false.

- [ ] **Step 4: Run server update/auth tests**

```bash
cd src-tauri
cargo test --no-default-features --features server origin_gate_tests -- --nocapture
cargo test --no-default-features --features server docker_ -- --nocapture
```

Expected: LAN/Tailscale synthetic owner tests pass, public synthetic owner is rejected, authenticated owner and Docker update tests pass.

- [ ] **Step 5: Commit remote-origin protection**

```bash
git add src-tauri/src/apihub.rs
git commit -m "fix: secure public remote update management"
```

---

### Task 5: Verify Docker/NAS replacement and frontend recovery

**Files:**
- Modify only if a new red test exposes a defect: `src/composables/useUpdater.ts`, `src/composables/useUpdater.test.ts`, `docker/test-update-e2e.sh`, `src-tauri/src/apihub.rs`

**Interfaces:**
- Consumes: existing `requestId`, `bootId`, `targetRevision`, `/api/build`, `/api/ready` and finite request states.
- Produces: no new public interface unless a reproduced failure requires one.

- [ ] **Step 1: Extend frontend tests for wrong revision and explicit terminal failure**

Extract only the minimum pure decision helper needed from `waitForDockerReplacement`, for example:

```ts
export function replacementMatches(
  build: DockerBuild | null,
  sourceBootId: string,
  targetRevision: string,
): boolean {
  return Boolean(
    build?.bootId &&
      build.bootId !== sourceBootId &&
      build.buildRevision === targetRevision,
  );
}
```

Test that same boot, changed boot with wrong revision, and missing build are false; changed boot with exact revision is true.

- [ ] **Step 2: Run frontend and Rust Docker tests**

```bash
npx vitest run src/composables/useUpdater.test.ts
cd src-tauri
cargo test --no-default-features --features server docker_ -- --nocapture
```

Expected: all focused tests pass after the helper is integrated into the polling loop.

- [ ] **Step 3: Run disposable Docker E2E when Docker is available**

```bash
docker build -t polaris-update-e2e-base:local .
POLARIS_E2E=1 BASE_IMAGE=polaris-update-e2e-base:local sh docker/test-update-e2e.sh
```

Expected: wrong token reaches `failed`, unavailable registry reaches a finite failure, happy path observes a new boot plus exact target revision and ready state, data volume and runtime config survive.

If the host lacks Docker, do not mark this step passed locally; rely on the unchanged GitHub `docker-image.yml` gated E2E and report it as pending until CI completes.

- [ ] **Step 4: Commit only if this task changed code**

```bash
git add src/composables/useUpdater.ts src/composables/useUpdater.test.ts docker/test-update-e2e.sh src-tauri/src/apihub.rs
git diff --cached --quiet || git commit -m "test: harden remote update recovery evidence"
```

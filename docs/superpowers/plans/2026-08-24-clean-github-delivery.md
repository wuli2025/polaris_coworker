# Clean GitHub Delivery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reconstruct the Polaris feature on current `main` without line-ending noise, verify both repositories, obtain independent review, and update the existing GitHub PRs without overwriting concurrent work.

**Architecture:** Build in a second ignored worktree rooted at current `origin/main`, replay every functional commit except the pure CRLF snapshot, then add doctor and updater fixes. Push the verified clean history to the existing PR branch with an exact `--force-with-lease`; keep the user's dirty original worktree untouched.

**Tech Stack:** Git worktrees, Cargo, npm/Vitest, Node.js, GitHub REST/Actions, Rust/Windows builds.

**Spec:** `docs/superpowers/specs/2026-08-24-remote-update-and-delivery-stabilization-design.md`

## Global Constraints

- Never stash, restore, reset, stage, or commit the original worktree's `package-lock.json` modification.
- Skip commit `58f7aaf` because it is exactly 43 files and 21,419 added/21,419 removed lines of pure CRLF churn.
- Do not merge to `main`, create a tag, or publish a production Release.
- Force-update only `origin/feat/polaris-agent-stress`, using an exact expected remote SHA.
- Stop rather than overwrite if the remote branch changes after the expected SHA is captured.
- Keep both PR worktrees after pushing for review feedback.

---

### Task 1: Reconstruct a clean Polaris integration branch

**Files:**
- No source edits in this task.
- Worktree: `.worktrees/polaris-agent-delivery-clean`
- Branch: `feat/polaris-agent-delivery-clean`

**Interfaces:**
- Consumes functional commits `c2d317c`, `fdac165`, `b6d9e38`, `2d941fe`, `498db9f`, `d22a465`, `d0ac2a7`, `56a3aa7`, `6eea0ff`, `8282c9b`, `13d72e1`, `166bacf`, and `98922d7`, plus the commit whose subject is `docs: plan full-path remote update delivery`.
- Produces a clean branch based on current `origin/main` for the other two implementation plans.

- [ ] **Step 1: Confirm worktree isolation and ignored directory**

```bash
git rev-parse --show-toplevel
git rev-parse --git-dir
git rev-parse --git-common-dir
git check-ignore -q .worktrees
```

Expected: the current workspace is a linked worktree and `.worktrees` is ignored.

- [ ] **Step 2: Fetch and capture immutable delivery inputs**

```bash
git fetch --prune origin
git rev-parse origin/main
git rev-parse origin/feat/polaris-agent-stress
git status --short --branch
```

Record the remote feature SHA in the execution log for the final lease. The only modification in the original worktree must remain `package-lock.json`.

- [ ] **Step 3: Create the clean worktree from current main**

```bash
git worktree add .worktrees/polaris-agent-delivery-clean \
  -b feat/polaris-agent-delivery-clean origin/main
```

- [ ] **Step 4: Replay functional commits while ignoring EOL context**

From the clean worktree:

```bash
git cherry-pick -Xignore-space-at-eol \
  c2d317c fdac165 b6d9e38 2d941fe 498db9f d22a465 d0ac2a7 \
  56a3aa7 6eea0ff 8282c9b 13d72e1 166bacf 98922d7
plan_commit="$(git log feat/polaris-agent-stress -1 --format=%H \
  --grep='^docs: plan full-path remote update delivery$')"
test -n "$plan_commit"
git cherry-pick -Xignore-space-at-eol "$plan_commit"
```

If `package.json` conflicts, retain all current `main` NAS/release scripts and add the three Agent scripts: `test:agent-stress`, `test:server-release`, and `stress:agent`. If any CRLF-converted source conflicts while replaying commit `166bacf`, reconstruct only the semantic hunks shown by `git diff --ignore-space-at-eol 166bacf^ 166bacf`.

- [ ] **Step 5: Prove the CRLF snapshot is absent and mergeability is clean**

```bash
! git merge-base --is-ancestor 58f7aaf HEAD
git merge-tree --write-tree --name-only origin/main HEAD
git diff --check origin/main...HEAD
```

Expected: `58f7aaf` is not an ancestor, `merge-tree` reports no conflict, and diff check exits 0.

- [ ] **Step 6: Confirm the known baseline and implementation inputs**

```bash
npm run test:agent-stress
cd src-tauri
cargo test -p polaris-kernel doctor:: -- --nocapture
```

Expected before the doctor plan: stress harness passes; doctor reports exactly the four previously documented failures and no additional failure.

---

### Task 2: Run the complete local verification matrix

**Files:**
- No planned source edits; failures must return to the relevant TDD task.

**Interfaces:**
- Consumes completed doctor and remote-update plans.
- Produces fresh verification evidence for code review and GitHub delivery.

- [ ] **Step 1: Verify formatting and repository diff hygiene**

```bash
git diff --check origin/main...HEAD
git status --short --branch
git diff --name-only origin/main...HEAD | sort
```

Expected: no whitespace errors, no unplanned dirty files, and no `package-lock.json` change from this feature.

- [ ] **Step 2: Run Polaris frontend, stress, and production build**

```bash
npm run test:unit
npm run test:agent-stress
npm run build
```

Expected: all commands exit 0.

- [ ] **Step 3: Run Polaris Rust suites**

```bash
cd src-tauri
cargo test -p polaris-kernel --lib
cargo test -p polaris-kernel chat::pipeline::tests:: -- --nocapture
cargo test --no-default-features --features server docker_ -- --nocapture
cargo test --no-default-features --features server server_exit_cleanup_kills_registered_agent_children_on_early_error -- --nocapture
cargo check -p polaris-cli
```

Expected: zero failures. Explicitly ignored diagnostic tests remain ignored.

- [ ] **Step 4: Verify server release artifacts and manifest generator**

```bash
cd ../
cargo build --release -p polaris-cli --bins --features collab-net
node scripts/verify-server-release.mjs
npx vitest run scripts/release-manifest.test.mjs
```

Expected: server, Forge and skills are present; release manifest tests pass.

- [ ] **Step 5: Verify i-agent independently**

```bash
cd /mnt/d/polaris/i-agent/i-agent
cargo test --all-targets
cargo test --test stdio_integration
cargo build --release
powershell.exe -NoProfile -Command "Set-Location 'D:\polaris\i-agent\i-agent'; cargo build --release"
```

Expected: all i-agent tests and release build pass.

- [ ] **Step 6: Run the representative Agent regression matrix**

Start the verified server in one execution session with:

```bash
POLARIS_FAST_AGENT_BIN=/mnt/d/polaris/i-agent/i-agent/target/release/i-agent.exe \
POLARIS_WEB_DIR="$PWD/dist" \
POLARIS_PORT=8899 \
src-tauri/target/release/polaris-server
```

In the harness session, inject `POLARIS_STRESS_MINIMAX_TOKEN` only from the existing secure runtime provider store, then run:

```bash
node scripts/agent-stress/run.mjs matrix \
  --providers stress-minimax-m3 \
  --scenarios browser-catalog-first-page,ppt-12-executive,code-next-order-id,browser-layout-change,browser-slow-response,code-quantity-summary \
  --work-mode fast \
  --base-url http://127.0.0.1:8899
```

Expected: six scenarios pass with exactly one terminal result per run. Stop the server through its normal signal path before the residual-process check.

- [ ] **Step 7: Verify no residual processes or secrets**

Check Windows and WSL for i-agent, Polaris server, Forge, browser runner, Playwright and CloakBrowser test processes. Scan `git diff origin/main...HEAD` and generated reports for complete `sk-`, `ANTHROPIC_AUTH_TOKEN` values and known user tokens; only redacted placeholders may appear.

Expected: zero residual related processes and no complete credential.

---

### Task 3: Obtain independent code review and address findings

**Files:**
- Modify only files required by valid Critical or Important findings.

**Interfaces:**
- Consumes: `BASE_SHA=$(git rev-parse origin/main)` and `HEAD_SHA=$(git rev-parse HEAD)`.
- Produces: independent review verdict and any reviewed corrective commits.

- [ ] **Step 1: Dispatch a code reviewer with exact scope**

Provide the reviewer the spec, all three plans, base/head SHAs, known dirty-file exclusion, update security requirements, desktop protocol requirements, and test evidence. Request prioritized Critical/Important/Minor findings and a merge-readiness verdict.

- [ ] **Step 2: Reproduce every Critical or Important finding**

For each finding, run the focused test or static trace proving whether it is valid. Do not change code for unsupported opinions.

- [ ] **Step 3: Fix valid findings with red-green tests**

Add a focused failing regression test, run it red, implement the minimum correction, rerun focused and domain suites, and commit:

After confirming `git status --short` contains only the validated review fix and its regression test, commit with:

```bash
git add -A
git diff --cached --check
git commit -m "fix: address remote update review findings"
```

If there are no valid Critical/Important findings, do not create an empty commit.

- [ ] **Step 4: Request final reviewer verdict**

Provide the new head SHA and focused verification evidence. Required verdict: no unresolved Critical or Important finding and explicit merge readiness.

---

### Task 4: Update existing GitHub PRs safely

**Files:**
- No source edits.
- Remote branches: `feat/polaris-agent-stress`, `feat/polaris-stdio-fast-agent`.

**Interfaces:**
- Consumes: clean verified Polaris HEAD, recorded expected remote feature SHA, and clean i-agent HEAD.
- Produces updated existing PRs with verification evidence.

- [ ] **Step 1: Re-fetch and verify the force-with-lease precondition**

```bash
git fetch origin
git rev-parse origin/feat/polaris-agent-stress
```

Expected: the SHA exactly matches the value recorded in Task 1. If it differs, stop and inspect the new remote commits.

- [ ] **Step 2: Push clean Polaris history with an exact lease**

```bash
git push origin \
  --force-with-lease=refs/heads/feat/polaris-agent-stress:166bacfdf85dd6366371cca4c3c1e7318c465cae \
  HEAD:refs/heads/feat/polaris-agent-stress
```

This exact lease is the audited remote head from 2026-08-23. If the remote branch has moved, the push must fail and execution must stop for inspection. Never use plain `--force`.

- [ ] **Step 3: Push i-agent only if its branch gained commits**

```bash
git -C /mnt/d/polaris/i-agent/i-agent push origin feat/polaris-stdio-fast-agent
```

- [ ] **Step 4: Update PR descriptions through GitHub API**

Update Polaris PR 1 and i-agent PR 1 with: scope, remote update matrix, doctor fix, exact test counts, representative matrix result, branch-history cleanup, known ignored tests, no-release boundary, and companion PR link. Do not include credentials or local provider paths.

- [ ] **Step 5: Verify PR state and checks**

Query both PRs and head check-runs. Expected: Polaris PR is mergeable and not dirty; i-agent PR remains mergeable; all emitted CI checks reach success. If GitHub has no Polaris check-run for a required local-only test, preserve the local command and output count in the PR description.

- [ ] **Step 6: Verify final remote SHAs**

```bash
git ls-remote origin refs/heads/feat/polaris-agent-stress
git -C /mnt/d/polaris/i-agent/i-agent ls-remote origin refs/heads/feat/polaris-stdio-fast-agent
```

Expected: each remote SHA equals the locally verified head for that repository.

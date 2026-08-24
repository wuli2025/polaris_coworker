# Doctor PATH Portability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Polaris doctor PATH handling correct and deterministic on Windows, macOS, and Linux, eliminating the four known cross-platform test failures without hiding product defects.

**Architecture:** Centralize process-PATH separator and normalization semantics in `doctor/path.rs`, then express install-root fixtures using paths native to the test platform. Windows remains case-insensitive and semicolon-delimited; Unix remains case-sensitive and colon-delimited.

**Tech Stack:** Rust 2021, `std::env`, `std::path`, Cargo test.

**Spec:** `docs/superpowers/specs/2026-08-24-remote-update-and-delivery-stabilization-design.md`

## Global Constraints

- Run this plan in the clean integration worktree created by `2026-08-24-clean-github-delivery.md` Task 1.
- Do not stage or modify the original worktree's `package-lock.json`.
- Every product change follows red-green TDD.
- Tests that mutate `PATH` must restore it even after a panic.
- Do not silence failures with a blanket `#[cfg(windows)]` when current-platform behavior can be tested.

---

### Task 1: Make process PATH parsing platform-correct

**Files:**
- Modify: `src-tauri/crates/polaris-kernel/src/doctor/path.rs:43-69`
- Test: `src-tauri/crates/polaris-kernel/src/doctor/path.rs:399-470`

**Interfaces:**
- Consumes: `std::env::consts`, `std::env::{var_os, set_var, remove_var}`.
- Produces: `fn process_path_separator() -> char`, `fn normalize_process_path_dir(&str) -> String`, and corrected `path_contains_dir(&str, &str) -> bool`.

- [ ] **Step 1: Add failing separator and case-sensitivity tests**

Add platform-specific assertions and an environment guard in the existing test module:

```rust
struct PathEnvGuard(Option<std::ffi::OsString>);

impl PathEnvGuard {
    fn capture() -> Self {
        Self(std::env::var_os("PATH"))
    }
}

impl Drop for PathEnvGuard {
    fn drop(&mut self) {
        match self.0.take() {
            Some(value) => std::env::set_var("PATH", value),
            None => std::env::remove_var("PATH"),
        }
    }
}

#[test]
#[cfg(not(windows))]
fn process_path_uses_colons_and_case_sensitive_entries_on_unix() {
    assert!(path_contains_dir("/opt/Polaris/bin:/usr/bin", "/opt/Polaris/bin"));
    assert!(!path_contains_dir("/opt/Polaris/bin:/usr/bin", "/opt/polaris/bin"));
    assert!(!path_contains_dir("/opt/Polaris/bin;/usr/bin", "/usr/bin"));
}

#[test]
#[cfg(windows)]
fn process_path_uses_semicolons_and_case_insensitive_entries_on_windows() {
    assert!(path_contains_dir(
        r"C:\Program Files\Polaris;C:\Windows",
        r"c:\program files\polaris\",
    ));
}
```

- [ ] **Step 2: Run the focused tests and confirm the Unix test fails**

Run:

```bash
cd src-tauri
cargo test -p polaris-kernel doctor::path::tests::process_path_ -- --nocapture
```

Expected on Linux/WSL: FAIL because `path_contains_dir` splits only on `;` and lowercases Unix paths.

- [ ] **Step 3: Implement platform-aware separator and normalization**

Replace the fixed Windows semantics with:

```rust
fn process_path_separator() -> char {
    if cfg!(windows) { ';' } else { ':' }
}

fn normalize_process_path_dir(value: &str) -> String {
    let trimmed = value.trim().trim_end_matches(['\\', '/']);
    if cfg!(windows) {
        trimmed.to_lowercase()
    } else {
        trimmed.to_string()
    }
}

pub(crate) fn path_contains_dir(path_str: &str, dir: &str) -> bool {
    let target = normalize_process_path_dir(dir);
    if target.is_empty() {
        return false;
    }
    path_str
        .split(process_path_separator())
        .any(|part| normalize_process_path_dir(part) == target)
}
```

Use `process_path_separator()` in `prepend_process_path` instead of repeating a second separator decision.

- [ ] **Step 4: Make the PATH mutation test use a native marker and restore PATH**

At the start of `prime_and_prepend_behaviour`, capture `_path_guard`. Replace the Windows drive marker with a current-platform path:

```rust
let _path_guard = PathEnvGuard::capture();
let marker = std::env::temp_dir()
    .join("polaris-test-marker-dir-do-not-exist")
    .to_string_lossy()
    .to_string();
let first = prepend_process_path(&marker);
```

Pass `&marker` to subsequent assertions and calls.

- [ ] **Step 5: Run path tests and confirm they pass**

Run:

```bash
cd src-tauri
cargo test -p polaris-kernel doctor::path::tests:: -- --nocapture
```

Expected: all `doctor::path::tests` pass on the current platform.

- [ ] **Step 6: Commit the PATH fix**

```bash
git add src-tauri/crates/polaris-kernel/src/doctor/path.rs
git commit -m "fix: make doctor PATH handling platform aware"
```

---

### Task 2: Make install-path fixtures native to each test platform

**Files:**
- Modify: `src-tauri/crates/polaris-kernel/src/doctor/path.rs:413-428`
- Modify: `src-tauri/crates/polaris-kernel/src/doctor/verify.rs:1100-1180`

**Interfaces:**
- Consumes: existing `claude_dir_from_path`, `install_root`, and `install_conflicts` functions.
- Produces: `fn test_install_paths() -> (PathBuf, PathBuf, PathBuf)` inside `verify.rs` tests and native fixtures in `path.rs` tests.

- [ ] **Step 1: Replace the path.rs Windows-only literals with native composed paths**

Use components so `std::path::Path` interprets the fixture on every host:

```rust
#[test]
fn claude_dir_from_path_picks_parent_or_npm_prefix() {
    let native = PathBuf::from("polaris-test-home")
        .join(".local")
        .join("bin")
        .join(if cfg!(windows) { "claude.exe" } else { "claude" });
    assert_eq!(claude_dir_from_path(&native), native.parent().map(|path| path.to_path_buf()));

    let npmish = PathBuf::from("polaris-test-npm")
        .join("node_modules")
        .join("@anthropic-ai")
        .join("claude-code")
        .join("bin")
        .join(if cfg!(windows) { "claude.exe" } else { "claude" });
    let dir = claude_dir_from_path(&npmish).expect("应解析出某个目录");
    assert!(!dir.ends_with(if cfg!(windows) { "claude.exe" } else { "claude" }));
}
```

Import `Path` in the test module through the existing `super::*` scope or use `std::path::Path::to_path_buf` explicitly.

- [ ] **Step 2: Add a shared native install fixture in verify.rs tests**

```rust
fn test_install_paths() -> (PathBuf, PathBuf, PathBuf) {
    let prefix = PathBuf::from("polaris-test-npm");
    let shim = prefix.join(if cfg!(windows) { "claude.cmd" } else { "claude" });
    let npm_native = prefix
        .join("node_modules")
        .join("@anthropic-ai")
        .join("claude-code")
        .join("bin")
        .join(if cfg!(windows) { "claude.exe" } else { "claude" });
    let local = PathBuf::from("polaris-test-home")
        .join(".local")
        .join("bin")
        .join(if cfg!(windows) { "claude.exe" } else { "claude" });
    (shim, npm_native, local)
}
```

Update `install_root_groups_npm_entries_together`, `single_npm_install_reports_no_conflict`, and `two_installs_with_different_versions_report_both_conflicts` to consume these three paths instead of drive-letter strings.

- [ ] **Step 3: Run the four originally failing tests**

Run each filter separately so Cargo accepts one test filter per invocation:

```bash
cd src-tauri
cargo test -p polaris-kernel claude_dir_from_path_picks_parent_or_npm_prefix -- --nocapture
cargo test -p polaris-kernel prime_and_prepend_behaviour -- --nocapture
cargo test -p polaris-kernel install_root_groups_npm_entries_together -- --nocapture
cargo test -p polaris-kernel two_installs_with_different_versions_report_both_conflicts -- --nocapture
```

Expected: all four pass.

- [ ] **Step 4: Run the full doctor and kernel suites**

```bash
cd src-tauri
cargo test -p polaris-kernel doctor:: -- --nocapture
cargo test -p polaris-kernel --lib
```

Expected: zero failures; explicitly ignored diagnostic tests remain ignored.

- [ ] **Step 5: Commit native path fixtures**

```bash
git add src-tauri/crates/polaris-kernel/src/doctor/path.rs src-tauri/crates/polaris-kernel/src/doctor/verify.rs
git commit -m "test: make doctor install fixtures cross-platform"
```

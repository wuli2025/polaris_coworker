/// 容器内更新脚本的固定路径（Dockerfile 把仓库根 update.sh COPY 到这里）。
/// 整段只在 server 壳编译 —— 桌面壳走 Tauri updater，用不到（否则 dead_code 警告）。
#[cfg(not(feature = "desktop"))]
const UPDATE_SCRIPT: &str = "/usr/local/bin/update.sh";
#[cfg(not(feature = "desktop"))]
const UPDATE_SERVICE_URL: &str = "http://polaris-updater:8080/v1/update";
#[cfg(not(feature = "desktop"))]
const DEFAULT_IMAGE_REPO: &str = "ghcr.io/wuli2025/polaris_coworker";
#[cfg(not(feature = "desktop"))]
const BUILD_REVISION_FILE: &str = "/app/polaris-build-revision";
#[cfg(not(feature = "desktop"))]
const BUILD_VERSION_FILE: &str = "/app/polaris-build-version";

#[cfg(not(feature = "desktop"))]
fn nonempty_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

#[cfg(not(feature = "desktop"))]
fn nonempty_file(path: &str) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

#[cfg(not(feature = "desktop"))]
fn select_build_value(marker: Option<String>, env: Option<String>, fallback: &str) -> String {
    marker.or(env).unwrap_or_else(|| fallback.to_string())
}

#[cfg(not(feature = "desktop"))]
fn current_build_revision() -> String {
    static REVISION: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    REVISION
        .get_or_init(|| {
            // Watchtower 会保留旧容器的运行时 Env；若只读 ENV，新镜像可能仍显示旧 revision。
            // 镜像文件系统里的 marker 随 image 一起替换，才是当前运行 build 的权威证据。
            select_build_value(
                nonempty_file(BUILD_REVISION_FILE),
                nonempty_env("POLARIS_BUILD_REVISION"),
                "local",
            )
        })
        .clone()
}

#[cfg(not(feature = "desktop"))]
fn current_build_version() -> String {
    static VERSION: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    VERSION
        .get_or_init(|| {
            select_build_value(
                nonempty_file(BUILD_VERSION_FILE),
                nonempty_env("POLARIS_BUILD_VERSION").or_else(|| nonempty_env("POLARIS_VERSION")),
                env!("CARGO_PKG_VERSION"),
            )
        })
        .clone()
}

/// 每个 server 进程一枚稳定、重启必变的 id。公开给 `/api/build` 与更新状态合并使用；
/// 它不含密钥或部署信息，只证明“浏览器看到的是不是同一个进程”。
#[cfg(not(feature = "desktop"))]
pub(crate) fn docker_build_identity() -> Value {
    static BOOT_ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    let boot = BOOT_ID.get_or_init(|| {
        let started = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        format!("{}-{started}", std::process::id())
    });
    json!({
        "bootId": boot,
        "version": current_build_version(),
        "buildRevision": current_build_revision(),
    })
}


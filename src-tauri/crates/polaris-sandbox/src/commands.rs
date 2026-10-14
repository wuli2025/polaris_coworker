//! 板块 ⑤ 安全沙箱层 — MVP (轻量 Docker 镜像 + CLI 包装)
//!
//! 设计依据: PRD-v6 §11
//! - 镜像基于 alpine:3.20, 仅装 node + claude-code
//! - 容器名固定 polaris-sandbox, 长驻 sleep infinity, exec 调起 claude
//! - 资源限制: memory=4g cpus=2
//! - 仅目标项目挂载 /workspace (读写)，知识库挂载 /kb (只读)
//!
//! ## 板块边界 (架构重构 Phase 1)
//! 本板块已抽离为独立 crate。它**不再** `use crate::kb` —— 挂载 KB 时所需的
//! 根路径改由 host 通过 [`polaris_core::KbLocator`] 注入 (依赖反转)，故本 crate
//! 只依赖 `polaris-core` 契约，可独立 `cargo test -p polaris-sandbox`。
//!
//! MVP 缩水:
//! - 不用 bollard, 直接 std::process::Command 调 docker CLI (零运行时依赖)
//! - 不实现完整 audit_stream, 状态查询走 docker ps/inspect
//! - 网络保留默认 bridge (PRD 要求 --network=polaris-net 白名单, 留到 v0.2)

use anyhow::Result;
use directories::UserDirs;
use polaris_core::{KbLocator, SandboxStatus};
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use tauri::Manager;

pub const IMAGE_NAME: &str = "polaris-sandbox:alpine";
pub const CONTAINER_NAME: &str = "polaris-sandbox";

/// 把镜像构建材料(Dockerfile)拷贝到 ~/Polaris/sandbox/, 方便用户审计。
/// 由 host 在 setup 阶段调用 (从前的 `sandbox::init(app)`)。
pub fn init() -> Result<()> {
    let dir = data_dir()?.join("sandbox");
    std::fs::create_dir_all(&dir)?;
    let dockerfile = dir.join("Dockerfile");
    if !dockerfile.exists() {
        std::fs::write(&dockerfile, include_str!("templates/Dockerfile.sandbox"))?;
    }
    Ok(())
}

fn data_dir() -> Result<PathBuf> {
    let user = UserDirs::new().ok_or_else(|| anyhow::anyhow!("no user dir"))?;
    Ok(user.home_dir().join("Polaris"))
}

fn dockerfile_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("sandbox").join("Dockerfile"))
}

// ───────────────────────── Status ────────────────────────

/// 探测沙箱状态。
///
/// 同时是 `#[tauri::command]` 与普通 `pub fn`：板块① `chat` 在发送前会直接
/// 调 `polaris_sandbox::sandbox_status()` 做容器预检，故保留可直接调用。
///
/// `command(async)`：docker CLI(version/info/inspect/ps 连环探测)是阻塞调用，同步命令
/// 默认跑主线程会冻 UI；标 async 让 tauri 调度到独立线程，fn 本体保持同步、直调不受影响。
#[tauri::command(async)]
pub fn sandbox_status() -> SandboxStatus {
    let mut notes = Vec::new();

    let docker_installed = Command::new("docker").arg("--version").output().is_ok();
    let docker_running = if docker_installed {
        match Command::new("docker").arg("info").output() {
            Ok(o) => o.status.success(),
            Err(_) => false,
        }
    } else {
        false
    };

    let image_built = if docker_running {
        match Command::new("docker")
            .args(["image", "inspect", IMAGE_NAME])
            .output()
        {
            Ok(o) => o.status.success(),
            Err(_) => false,
        }
    } else {
        false
    };

    let container_running = if docker_running {
        match Command::new("docker")
            .args([
                "ps",
                "--filter",
                &format!("name=^{}$", CONTAINER_NAME),
                "--format",
                "{{.Names}}",
            ])
            .output()
        {
            Ok(o) => String::from_utf8_lossy(&o.stdout).trim().eq(CONTAINER_NAME),
            Err(_) => false,
        }
    } else {
        false
    };

    if !docker_installed {
        notes.push("Docker CLI 未检测到。请先安装 Docker Desktop (Windows)。".into());
    } else if !docker_running {
        notes.push(
            "Docker daemon 未运行。请启动 Docker Desktop, 然后回到本页点击 \"刷新状态\"。".into(),
        );
    } else if !image_built {
        notes.push(format!(
            "镜像 {} 未构建。点击 \"构建镜像\" 拉取 alpine:3.20 + apk 装 claude-code 原生包 (无 Node)。约需 1-3 分钟。",
            IMAGE_NAME
        ));
    } else if !container_running {
        notes.push("镜像已就绪。点击 \"启动容器\" 拉起长驻沙箱(sleep infinity)。".into());
    }
    if docker_running && crate::lifecycle::inspect(CONTAINER_NAME).is_ok_and(|info| info.is_some()) {
        match validate_current_container() {
            Ok(()) => notes.push("沙箱挂载与安全基线检查通过。工具路由仍需校验本轮目标项目。".into()),
            Err(error) => notes.push(error),
        }
    }

    SandboxStatus {
        docker_installed,
        docker_running,
        image_built,
        image_name: IMAGE_NAME.into(),
        container_running,
        container_name: CONTAINER_NAME.into(),
        notes,
    }
}

// ───────────────────────── Build / Start / Stop ──────────

/// `command(async)`：docker build 约需 1-3 分钟,绝不能同步钉在主线程(见 sandbox_status 注)。
#[tauri::command(async)]
pub fn sandbox_build_image() -> Result<String, String> {
    let df = dockerfile_path().map_err(|e| e.to_string())?;
    if !df.exists() {
        return Err(format!("Dockerfile 不存在: {}", df.display()));
    }
    let ctx = df.parent().unwrap();
    let out = Command::new("docker")
        .args([
            "build",
            "-t",
            IMAGE_NAME,
            "-f",
            df.to_str().unwrap(),
            ctx.to_str().unwrap(),
        ])
        .output()
        .map_err(|e| format!("docker build 启动失败: {}", e))?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    if out.status.success() {
        Ok(format!(
            "[build ok] 镜像 {} 已构建。\n--- docker stdout (尾部) ---\n{}",
            IMAGE_NAME,
            tail(&stdout, 1200)
        ))
    } else {
        Err(format!(
            "docker build 失败 (exit={:?}):\n{}\n{}",
            out.status.code(),
            tail(&stderr, 1500),
            tail(&stdout, 800)
        ))
    }
}

/// 启动长驻沙箱容器。
///
/// 需要 KB 根路径来挂载 `/kb`。该路径不再 `use crate::kb` 直取，而是从 Tauri
/// 托管状态里取出 host 注入的 [`polaris_core::KbLocator`] 实现 (依赖反转)。
/// 仅作为命令被前端调用，故可安全接收注入的 `AppHandle`。
/// `command(async)`：docker ps/start/run 阻塞调用不进主线程(见 sandbox_status 注)。
#[tauri::command(async)]
pub fn sandbox_start(app: tauri::AppHandle, project_dir: Option<String>, rebuild: Option<bool>) -> Result<String, String> {
    let user = UserDirs::new().ok_or_else(|| "no user dir".to_string())?;
    let project_dir = project_dir.filter(|p| !p.trim().is_empty())
        .ok_or_else(|| "SANDBOX_PROJECT_REQUIRED: 请指定目标项目目录 (projectDir)，禁止挂载整个 Polaris。".to_string())?;
    let kb = app.state::<Arc<dyn KbLocator>>().kb_root();
    let config = crate::config::Config::load().map_err(|e| e.to_string())?;
    crate::lifecycle::start(&config, CONTAINER_NAME, IMAGE_NAME,
        std::path::Path::new(&project_dir), &kb, user.home_dir(), rebuild == Some(true))
        .map_err(|e| e.to_string())
}

/// `command(async)`：docker stop -t 5 + rm 可阻塞 5s+,同款不进主线程(见 sandbox_status 注)。
#[tauri::command(async)]
pub fn sandbox_stop() -> Result<String, String> {
    let stop = Command::new("docker")
        .args(["stop", "-t", "5", CONTAINER_NAME])
        .output()
        .map_err(|e| e.to_string())?;
    let rm = Command::new("docker")
        .args(["rm", "-f", CONTAINER_NAME])
        .output()
        .map_err(|e| e.to_string())?;
    Ok(format!(
        "[stop] stop_ok={} rm_ok={}",
        stop.status.success(),
        rm.status.success()
    ))
}

/// `command(async)`：docker exec 跑任意容器内命令,时长不可控,不进主线程(见 sandbox_status 注)。
#[tauri::command(async)]
pub fn sandbox_exec(cmd: String) -> Result<String, String> {
    validate_current_container()?;
    let parts = shell_split(&cmd);
    if parts.is_empty() {
        return Err("empty command".into());
    }
    let mut docker_args: Vec<&str> = vec!["exec", CONTAINER_NAME];
    let ref_parts: Vec<&str> = parts.iter().map(|s| s.as_str()).collect();
    docker_args.extend(ref_parts.iter().copied());
    let out = Command::new("docker")
        .args(&docker_args)
        .output()
        .map_err(|e| format!("docker exec 启动失败: {}", e))?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    if out.status.success() {
        Ok(stdout)
    } else {
        Err(format!(
            "exit={:?}\nstdout:\n{}\nstderr:\n{}",
            out.status.code(),
            stdout,
            stderr
        ))
    }
}

// ───────────────────────── helpers ───────────────────────

fn validate_current_container() -> Result<(), String> {
    let user = UserDirs::new().ok_or_else(|| "no user dir".to_string())?;
    let info = crate::lifecycle::inspect(CONTAINER_NAME).map_err(|e| e.to_string())?
        .ok_or_else(|| "SANDBOX_UNAVAILABLE".to_string())?;
    let config = crate::config::Config::load().map_err(|e| e.to_string())?;
    let mounts = info["Mounts"].as_array().ok_or_else(|| format!("SANDBOX_UNSAFE_MOUNT: missing required mount; {}", crate::policy::REBUILD_HINT))?;
    let root = |target: &str| -> Result<PathBuf, String> {
        mounts.iter().find(|m| m["Destination"] == target)
            .and_then(|m| m["Source"].as_str()).map(PathBuf::from)
            .ok_or_else(|| format!("SANDBOX_UNSAFE_MOUNT: missing required mount; {}", crate::policy::REBUILD_HINT))
    };
    config.validate_container(&info, &root("/workspace")?, &root("/kb")?, user.home_dir())
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn tail(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        let start = s.len() - n;
        let mut start = start;
        while start > 0 && !s.is_char_boundary(start) {
            start -= 1;
        }
        s[start..].to_string()
    }
}

fn shell_split(s: &str) -> Vec<String> {
    // 极简 split (空格分隔, 不支持 quoting); MVP 够用, 后续接 shlex
    s.split_whitespace().map(|w| w.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_split_basic() {
        assert_eq!(shell_split("claude --version"), vec!["claude", "--version"]);
        assert!(shell_split("   ").is_empty());
    }

    #[test]
    fn tail_respects_char_boundary() {
        // 不能在多字节字符中间切断
        let s = "你好世界";
        let t = tail(s, 5);
        assert!(s.ends_with(&t));
    }

    #[test]
    fn path_for_docker_normalizes_on_windows() {
        let p = std::path::Path::new("D:\\polaris\\PolarisKB");
        let out = crate::policy::path_for_docker(p);
        if cfg!(windows) {
            assert_eq!(out, "D:/polaris/PolarisKB");
        }
    }
}

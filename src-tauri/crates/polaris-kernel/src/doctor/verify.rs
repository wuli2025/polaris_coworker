//! 环境医生 · **深度校验**: 安装冲突扫描 + 「真的能跑」实测。
//!
//! `env_check` 回答的是「装没装」——**查得到文件就算就绪**。但真实机器上大量故障是
//! 「查得到、跑不了」或「这边跑得了、那边跑不了」:
//!
//! - **装重了**: npm 全局一份 + `~/.local/bin` 一份, 版本还不一样 —— 更新了其中一份,
//!   另一份仍被 PATH 先命中, 于是「更新完版本纹丝不动」;
//! - **终端与应用解析到不同的那份**: 应用启动时会预热进程 PATH (`prime_path_for_claude`),
//!   所以**应用能跑不等于用户在 CMD/PowerShell 里敲 `claude` 能跑** —— 反过来也一样;
//! - **Store 执行别名遮挡**: `%LOCALAPPDATA%\Microsoft\WindowsApps` 里的 0 字节占位符排在
//!   PATH 前面, 终端里敲下去直接跳应用商店;
//! - **外部 ANTHROPIC_\* 覆盖**: 用户自己在系统环境变量 / `~/.zshrc` 里设过 key 或 base_url,
//!   会盖掉应用内选的供应商, 表现为「面板选了 A, 实际走了 B」;
//! - **配置文件损坏**: `~/.claude.json` 半截 JSON → claude 每次启动即崩。
//!
//! 本模块的每一条结论都**真的起了子进程**去验, 而不是查文件在不在。三个层次:
//! ① `exec` —— 用解析出的绝对路径跑 `claude --version` (应用内 spawn 的同一条路径);
//! ② `terminal` —— **复现「新开一个终端」的 PATH 语义**再跑一次 (Windows 从注册表重建
//!    机器级+用户级 PATH 交给 `cmd.exe` 解析裸名; 类 Unix 把 PATH 压回系统最小集后走登录 shell),
//!    这才是「能不能在电脑终端里用」的唯一可信答案;
//! ③ `smoke` (deep) —— 真发一次最小请求 (`claude --print`), 端到端确证「真正可用」。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::path::*;
use super::probe::*;
use super::types::*;
use crate::runtime::procs::no_window;

#[path = "verify/helpers.rs"]
mod helpers;
use helpers::*;
#[path = "verify/steps.rs"]
mod steps;
use steps::*;
#[path = "verify/conflicts.rs"]
mod conflicts;
use conflicts::*;
// ───────────────────────── Command ─────────────────────────

/// 深度校验。`deep=true` 时额外真发一次最小请求做端到端冒烟 (会走网络、消耗极少量额度)。
///
/// 桌面端 async + spawn_blocking: 内部要串起十来个子进程探测 (含一次可能长达 2 分钟的冒烟),
/// 同步跑会把 Tauri 主线程钉死。server flavor 的 dispatch 本就在 spawn_blocking 中, 保持同步。
#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn env_verify(deep: bool) -> VerifyReport {
    tauri::async_runtime::spawn_blocking(move || env_verify_sync(deep))
        .await
        .unwrap_or_else(|_| env_verify_sync(deep))
}
#[cfg(not(feature = "desktop"))]
pub fn env_verify(deep: bool) -> VerifyReport {
    env_verify_sync(deep)
}

/// 同步版本体。桌面 flavor 下 [`env_verify`] 是 async(见上),所以**不带运行时的调用方**
/// (examples / 测试 / 脚本)必须走这条,否则拿到的是个 Future,取字段即编译不过。
pub fn env_verify_sync(deep: bool) -> VerifyReport {
    if !super::pi::legacy_enabled() { return super::pi::verify(); }
    let os = std::env::consts::OS.to_string();
    let exe = resolve_claude_exe();

    let (app_runnable, exec_step) = step_exec(exe.as_deref());

    let t_term = Instant::now();
    let term = terminal_probe();
    let terminal_runnable = term.version.is_some();

    let broken = broken_config_files();

    let mut steps = vec![
        exec_step,
        step_terminal(&term, app_runnable, t_term),
        step_shell(),
        step_config(&broken),
        step_auth(),
    ];
    if deep {
        steps.push(step_smoke(exe.as_deref(), app_runnable));
    }

    let conflicts = scan_conflicts(exe.as_deref(), &term, &broken);
    let has_high = conflicts.iter().any(|c| c.severity == "high");
    let shell_ok = steps.iter().any(|s| s.key == "shell" && s.status == "ok");
    let smoke_bad = steps
        .iter()
        .any(|s| s.key == "smoke" && s.status == "fail");
    let ok = app_runnable && shell_ok && !has_high && !smoke_bad;

    let summary = if !app_runnable {
        "Claude Code 跑不起来 —— 先按上面的提示装好 / 修好。".to_string()
    } else if has_high {
        format!(
            "能跑, 但检测到 {} 处会真出问题的安装冲突, 建议先清理。",
            conflicts.iter().filter(|c| c.severity == "high").count()
        )
    } else if !shell_ok {
        "Claude Code 能起来, 但没有可用 shell —— 它执行命令的工具会全部失败。".to_string()
    } else if !terminal_runnable {
        "应用内一切正常; 但在电脑终端里敲 `claude` 还不可用 —— 点「修复 PATH」即可。".to_string()
    } else if smoke_bad {
        "环境本身没问题, 但真发请求失败 —— 多半是凭据 / 网络, 见冒烟那一项。".to_string()
    } else if deep {
        "全部通过 —— 应用内、电脑终端里都能跑, 且真发请求成功。".to_string()
    } else {
        "全部通过 —— 应用内、电脑终端里都能跑 (想确证能真发请求, 点「深度检测」)。".to_string()
    };

    VerifyReport {
        os,
        ok,
        app_runnable,
        terminal_runnable,
        deep,
        steps,
        conflicts,
        summary,
    }
}

#[cfg(test)]
#[path = "verify/tests.rs"]
mod tests;

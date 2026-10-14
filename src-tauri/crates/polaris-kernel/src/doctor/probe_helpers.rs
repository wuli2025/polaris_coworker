//! 探测原语: which/版本探测、候选路径、工具检测、claude/uv 解析、子进程环境净化 (纯移动)。

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::*;
use crate::runtime::procs::no_window;

pub(crate) fn home_dir() -> Option<PathBuf> {
    directories::UserDirs::new().map(|u| u.home_dir().to_path_buf())
}

pub(crate) fn to_fwd(p: &std::path::Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// 跑一个子命令, 最多等 `timeout`; 超时则 kill 子进程并返回 None。
/// 探测类调用 (npm view / npm prefix / 版本号 / where / 读注册表 PATH 等) 用它兜底:
/// 网络卡死或进程僵死时不让 `env_check` 的探测线程永久阻塞。
/// stdout/stderr 各由独立线程读到 EOF, 避免子进程写满管道反压自锁。
pub(crate) fn output_with_timeout(
    mut cmd: Command,
    timeout: Duration,
) -> Option<std::process::Output> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().ok()?;
    let mut out_pipe = child.stdout.take()?;
    let mut err_pipe = child.stderr.take()?;
    let (tx_o, rx_o) = std::sync::mpsc::channel();
    let (tx_e, rx_e) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = std::io::Read::read_to_end(&mut out_pipe, &mut b);
        let _ = tx_o.send(b);
    });
    std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = std::io::Read::read_to_end(&mut err_pipe, &mut b);
        let _ = tx_e.send(b);
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None; // 超时: 杀掉并放弃
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(_) => return None,
        }
    };
    let stdout = rx_o.recv().unwrap_or_default();
    let stderr = rx_e.recv().unwrap_or_default();
    Some(std::process::Output {
        status,
        stdout,
        stderr,
    })
}

/// 用 `where.exe`(Windows) / `which`(unix) 找出某命令的全部命中路径 (存在的才留)。
pub(crate) fn which_all(bin: &str) -> Vec<PathBuf> {
    #[cfg(windows)]
    let mut cmd = {
        let mut c = Command::new("where.exe");
        c.arg(bin);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = Command::new("which");
        c.args(["-a", bin]);
        c
    };
    cmd.stdin(Stdio::null());
    no_window(&mut cmd);
    let out = match output_with_timeout(cmd, Duration::from_secs(20)) {
        Some(o) => o,
        None => return Vec::new(),
    };
    if !out.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .collect()
}

/// 首个非空行 (优先 stdout, 个别工具把版本写到 stderr); 进程非零退出 → None。
pub(super) fn first_line(out: &std::process::Output) -> Option<String> {
    if !out.status.success() {
        return None;
    }
    let pick = |bytes: &[u8]| -> Option<String> {
        String::from_utf8_lossy(bytes)
            .lines()
            .map(|l| l.trim())
            .find(|l| !l.is_empty())
            .map(|s| s.to_string())
    };
    pick(&out.stdout).or_else(|| pick(&out.stderr))
}

/// Windows 上「这个命中能不能直接跑起来」的排序权重: `.exe` 可被 `Command::new` 直接 spawn;
/// `.cmd`/`.bat` 得过 `cmd /c`; **无扩展名**的(如 `C:\Program Files\nodejs\npm` 那个给 Git Bash
/// 用的 sh 脚本) 在 Windows 上压根不是可执行文件, 只配当最后兜底 —— 老代码「偏好 .exe, 否则取
/// where 的首个命中」正好在 npm 上踩这个坑: 首个命中就是那个 sh 脚本, 于是面板里 npm 那行显示
/// 的路径是个跑不起来的东西。类 Unix 无扩展名是常态, 恒 0。
pub(super) fn exec_rank(p: &std::path::Path) -> u8 {
    #[cfg(windows)]
    {
        match p
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
        {
            Some(e) if e == "exe" => 0,
            Some(e) if e == "cmd" || e == "bat" => 1,
            _ => 2,
        }
    }
    #[cfg(not(windows))]
    {
        let _ = p;
        0
    }
}

/// 用**解析出的绝对路径**构造一条可跑的命令 (已配好 stdin=null + 无窗口)。
/// Windows 上非 `.exe` 一律过 `cmd /c` —— `CreateProcessW` 只会补 `.exe`, 不认 `.cmd`/PATHEXT。
pub(crate) fn command_at(exe: &std::path::Path, args: &[&str]) -> Command {
    #[cfg(windows)]
    let mut cmd = {
        let is_exe = exe
            .extension()
            .map(|e| e.eq_ignore_ascii_case("exe"))
            .unwrap_or(false);
        if is_exe {
            let mut c = Command::new(exe);
            c.args(args);
            c
        } else {
            let mut c = Command::new("cmd");
            c.arg("/c").arg(exe).args(args);
            c
        }
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = Command::new(exe);
        c.args(args);
        c
    };
    cmd.stdin(Stdio::null());
    no_window(&mut cmd);
    cmd
}

/// 取某个**已解析出绝对路径**的工具的版本号。优先用它而非 `probe_version`(裸名):
/// 「装了但不在 PATH」是环境医生显式支持的状态 (面板有「已安装 (不在 PATH)」+「修复 PATH」),
/// 裸名探测在这种机器上必然拿不到版本号。
pub(crate) fn probe_version_at(exe: &std::path::Path, args: &[&str]) -> Option<String> {
    let out = output_with_timeout(command_at(exe, args), Duration::from_secs(20))?;
    first_line(&out)
}

/// 取某命令的版本号 (裸名走 PATH)。Windows 走 `cmd /c <bin> <args>` 以便正确解析 .exe/.cmd
/// (PATHEXT); 其余平台直接执行。仅作 `probe_version_at` 解析不出路径时的兜底。
pub(crate) fn probe_version(bin: &str, args: &[&str]) -> Option<String> {
    #[cfg(windows)]
    let mut cmd = {
        let mut c = Command::new("cmd");
        let mut full = vec!["/c".to_string(), bin.to_string()];
        full.extend(args.iter().map(|s| s.to_string()));
        c.args(full);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = Command::new(bin);
        c.args(args);
        c
    };
    cmd.stdin(Stdio::null());
    no_window(&mut cmd);
    let out = output_with_timeout(cmd, Duration::from_secs(20))?;
    first_line(&out)
}

/// 在 Node 安装目录候选里按文件名铺开成完整路径候选。
pub(super) fn exe_in_node_dirs(names: &[&str]) -> Vec<PathBuf> {
    node_dir_candidates()
        .into_iter()
        .flat_map(|d| names.iter().map(|n| d.join(n)).collect::<Vec<_>>())
        .collect()
}

/// Node.js 可执行文件候选 (给 detect 用)。
/// **必须给**: 老代码给 node/npm 传的是空候选, 于是「Node 确实装在 `C:\Program Files\nodejs`,
/// 但不在**本进程** PATH 里」时面板直接报「未安装」, 还劝用户重装一遍已经装好的 Node。
/// 这个状态在 Windows 上很常见 —— MSI/winget 只写注册表 PATH, **已在运行的进程 PATH 是快照,
/// 不会刷新**; app 被从一个尚无 Node 的上下文拉起也一样。
pub(crate) fn node_candidates() -> Vec<PathBuf> {
    exe_in_node_dirs(if cfg!(windows) {
        &["node.exe"]
    } else {
        &["node"]
    })
}

/// npm 可执行文件候选 (给 detect 用)。理由同 [`node_candidates`]。
pub(crate) fn npm_candidates() -> Vec<PathBuf> {
    exe_in_node_dirs(if cfg!(windows) {
        &["npm.cmd", "npm.exe"]
    } else {
        &["npm"]
    })
}

/// 解析一个「能直接跑」的 npm。PATH 命中按可执行性排序 (Windows: `npm.cmd` 优先于那个无扩展名的
/// sh 脚本), 全都不在 PATH 时退到 Node 安装目录候选 —— **用户刚装完 Node、本进程 PATH 尚未刷新**
/// 时也找得到, 不然「装了 Node 还说没 npm」。
pub(crate) fn resolve_npm_exe() -> Option<PathBuf> {
    let mut hits: Vec<PathBuf> = which_all("npm")
        .into_iter()
        .filter(|p| !is_app_exec_alias(p))
        .collect();
    hits.sort_by_key(|p| exec_rank(p)); // 稳定排序: 同级保持 where.exe 原序
    hits.into_iter()
        .next()
        .or_else(|| npm_candidates().into_iter().find(|p| p.exists()))
}

/// npm 全局安装前缀。走 `npm prefix -g` —— **用户可能改过前缀**(实测有人放在 `D:\Users\x\npm`,
/// 而非默认 `%APPDATA%\npm`), 硬编码默认值会漏掉。失败 / 目录不存在 → None。
pub(crate) fn npm_global_prefix() -> Option<PathBuf> {
    let npm = resolve_npm_exe()?;
    let out = output_with_timeout(command_at(&npm, &["prefix", "-g"]), Duration::from_secs(20))?;
    if !out.status.success() {
        return None;
    }
    let line = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim())
        .find(|l| !l.is_empty())?
        .to_string();
    let p = PathBuf::from(line);
    p.exists().then_some(p)
}

/// 某个 npm 全局前缀下 Claude Code 的**真·原生 exe** 路径
/// (`<prefix>/node_modules/@anthropic-ai/claude-code/bin/claude.exe`)。
/// postinstall 把平台二进制拷到这里; 这是可被 `Command::new` 直接 spawn 的目标,
/// 而 `<prefix>/claude.cmd` 只是调它的 shim。
pub(super) fn npm_claude_native_exe(prefix: &std::path::Path) -> PathBuf {
    prefix
        .join("node_modules")
        .join("@anthropic-ai")
        .join("claude-code")
        .join("bin")
        .join("claude.exe")
}

/// 已知的 claude 可执行文件候选位置。原生 `.exe` 优先 (能直接 spawn),
/// npm 的 `claude.cmd` shim 仅作探测 / PATH 兜底。
pub(crate) fn claude_candidates() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(h) = home_dir() {
        // 官方原生脚本: ~/.local/bin/claude(.exe)
        v.push(h.join(".local").join("bin").join("claude.exe"));
        v.push(h.join(".local").join("bin").join("claude"));
        // macOS 免 sudo 装的 Node (~/.local/polaris-node) 的全局 bin: `npm i -g` 把 claude
        // 链到这里。mac GUI 从 Finder 启动时 PATH 极简、`npm prefix -g` 又拿不到 → 显式兜底,
        // 让重启后 chat spawn 仍找得到。
        v.push(
            h.join(".local")
                .join("polaris-node")
                .join("bin")
                .join("claude"),
        );
        // Windows 便携版 Node (后台静默安装装的) 的全局前缀就是它自己的根目录 ——
        // `npm i -g` 把 claude 直接放这里, 没有 bin/ 这一层。必须显式列出:
        // 用户的 .npmrc 可能把 `npm prefix -g` 指到别处, 那条路探不到我们装的这份。
        let portable = h.join(".local").join("polaris-node");
        v.push(npm_claude_native_exe(&portable));
        v.push(portable.join("claude.exe"));
        v.push(portable.join("claude.cmd"));
    }
    // npm 全局 (用户真实前缀): 先原生 exe, 再 shim
    if let Some(prefix) = npm_global_prefix() {
        v.push(npm_claude_native_exe(&prefix));
        v.push(prefix.join("claude.exe"));
        v.push(prefix.join("claude.cmd"));
    }
    // 默认前缀兜底 (拿不到 `npm prefix -g` 时, 例如 npm 不在 PATH)
    if let Some(h) = home_dir() {
        let appdata_npm = h.join("AppData").join("Roaming").join("npm");
        v.push(npm_claude_native_exe(&appdata_npm));
        v.push(appdata_npm.join("claude.cmd"));
        v.push(appdata_npm.join("claude.exe"));
    }
    v
}


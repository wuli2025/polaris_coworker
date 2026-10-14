//! 流式安装管线 + 安装命令 (Claude/Node/PowerShell/uv) + 取消 (纯移动)。

#[cfg(not(feature = "desktop"))]
use crate::host::AppHandle;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
#[cfg(feature = "desktop")]
use tauri::{AppHandle, Emitter};

use super::check::*;
use super::path::*;
use super::probe::*;
use super::types::*;
use crate::runtime::procs::{no_window, CHILDREN};

static REQ_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(crate) fn next_req_id() -> String {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let c = REQ_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("env-{:x}-{:x}", ts, c)
}

/// 安装 Claude Code。method: `direct`/`npm` (默认, 直抓平台包 tgz, R2 优先多源) |
/// `native` (官方原生脚本 install.ps1/sh, 境外网络兜底)。
/// 流式把安装日志通过 `env:stream` 事件推给前端; 成功后自动修 PATH。
///
/// 默认路径**不经 npm** —— 见 `CLAUDE_VER` 说明: `npm i -g` 下那个 80MB 的 optional 依赖常卡死。
/// (为兼容旧前端, method 传 `"npm"` 也走这条直装路径, 不再真的调 npm。)
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn env_install_claude(app: AppHandle, method: Option<String>) -> Result<String, String> {
    super::pi::require_legacy()?;
    let method = method.unwrap_or_else(|| "direct".to_string());
    let req_id = next_req_id();
    // "native" = 官方原生脚本 (从 claude.ai/GCS 拉, 国内常被墙, 仅境外网络兜底);
    // 其余一律走「直抓平台包 tgz」: R2 优先、多源回落、不碰 npm。
    let cmd = if method == "native" {
        build_install_shell(&claude_native_cmd())
    } else {
        claude_install_command()
    };
    stream_install(app, req_id.clone(), cmd, true, "Claude Code");
    Ok(req_id)
}

/// 安装 Node.js LTS —— npm 安装方式的前置依赖。两端都走「国内镜像优先」, 装完把 bin 目录
/// 塞进进程 PATH (stream_install 收尾处), 故 `fix_path_after=false`。
///
/// - **Windows**: 两层策略 —— ① 有 winget 先用 winget; ② 缺失/失败 → 下载官方 MSI (npmmirror
///   镜像加速) 静默安装 (Win10 常无 winget, 故必须有 MSI 兜底)。
/// - **macOS**: 下载 Node 官方 darwin tar.gz (走 npmmirror 二进制镜像, 国内可达) **免 sudo**
///   解压到 `~/.local/polaris-node`, 并把其 `bin` 写进 shell 配置 —— 不动系统目录、不弹 UAC/密码。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn env_install_node(app: AppHandle) -> Result<String, String> {
    super::pi::require_legacy()?;
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = &app;
        return Err(
            "Node.js 自动安装目前支持 Windows 与 macOS; Linux 请用系统包管理器安装。".into(),
        );
    }
    #[cfg(any(windows, target_os = "macos"))]
    {
        let req_id = next_req_id();
        #[cfg(windows)]
        let cmd = build_powershell(&node_install_script());
        #[cfg(target_os = "macos")]
        let cmd = build_install_shell(MAC_NODE_INSTALL_SCRIPT);
        stream_install(app, req_id.clone(), cmd, false, "Node.js");
        Ok(req_id)
    }
}

/// macOS Node.js 安装脚本 (POSIX sh) —— 免 sudo: 下载官方 darwin tar.gz 解压到
/// `~/.local/polaris-node`, 把 `bin` 写进 zsh/bash 配置。下载走 npmmirror 二进制镜像 (国内可达),
/// 不行再退 nodejs.org 直连。选 20.x LTS, 与 Windows 一致。
#[cfg(target_os = "macos")]
const MAC_NODE_INSTALL_SCRIPT: &str = include_str!("install/mac_node_install_script.sh");

/// 三个 Windows 安装脚本 (PowerShell 7 / Node.js / uv) 共用的下载前奏 —— 两项环境设置 +
/// 一个带校验和重试的下载函数。
///
/// **① 关掉 IWR 的进度条**: 本应用以**无窗口**方式 spawn PowerShell、stdout 又被重定向进管道,
/// 这种上下文下 Windows PowerShell 5.1 的进度条渲染会把 `Invoke-WebRequest` 拖慢一个数量级 ——
/// 同机同镜像实测 584 KB/s → 4266 KB/s (**7.3 倍**)。~107MB 的 PowerShell MSI 因此在网速一般的
/// 机器上会一路摸到 `-TimeoutSec` 上限而失败。**这是「PowerShell 7 有些概率装不上」的主因**:
/// 成败取决于当时网速, 所以时好时坏。
///
/// **② 显式并入 TLS 1.2**: 老的 / 受管的机器上 5.1 的默认安全协议可能仍是 TLS 1.0, 而 GitHub
/// 与各镜像早已只收 TLS 1.2+ → 握手阶段直接失败。
///
/// **③ 下载校验**: 国内 GitHub 代理挂掉时常常回「200 + 一个 HTML/JSON 错误页」, 老代码只看
/// `Length -gt 1MB`, 于是把错误页当安装包喂给 msiexec, 报一句用户看不懂的错。这里改成校验
/// **体积下限 + 文件头魔数**, 且整轮镜像都没成会再重试一轮 (连接被重置在国内很常见)。
const PS_DOWNLOAD_PRELUDE: &str = include_str!("install/ps_download_prelude.ps1");

/// msiexec 静默安装的共用封装 (PowerShell 7 / Node.js 两个 MSI 都用)。
/// 装到 Program Files 要管理员 → `RunAs` 触发 UAC (用户拒绝则友好报错, 不静默失败)。
/// **1618 = 「另一个安装正在进行」** (撞上 Windows Update 或别的 MSI) —— 这是典型的**概率性**
/// 失败, 老代码直接判死, 这里等一会儿重试, 最多三次。结果写 $global:PolarisMsiCode。
const PS_MSIEXEC_HELPER: &str = include_str!("install/ps_msiexec_helper.ps1");

/// MSI = OLE 复合文档, 文件头魔数固定为 `D0CF11E0A1B11AE1`。
const MSI_MAGIC: &str = "d0cf11e0a1b11ae1";
/// zip 文件头魔数 (`PK\x03\x04`) —— uv 的 release 包。
const ZIP_MAGIC: &str = "504b0304";
/// gzip 文件头魔数 (`\x1f\x8b`) —— npm 平台包 tgz (Claude Code)。
const GZ_MAGIC: &str = "1f8b";

/// 自家依赖包的分发基址 —— Cloudflare `/downloads/*` 由 `functions/downloads/[[path]].js`
/// 从 R2 桶 `polaris-downloads` 流式取出 (支持 Range、边缘缓存 24h、出站免费)。
/// 对应的 R2 key 前缀是 `deps/`(**不含** `downloads/`, 那是路由前缀不是 key 的一部分)。
///
/// 传新包: `wrangler r2 object put polaris-downloads/deps/<文件名> --file <本地路径> --remote`
/// 传完务必对着生产域名核字节数与文件头魔数 —— 与发版终验同一套规矩。
const DEPS_BASE: &str = "https://llmwiki.cloud/downloads/deps";

/// 这三个版本号**同时**决定「下载哪个官方包」与「R2 里该有哪个包」——
/// 改任何一个都必须把对应文件传进 R2 的 `deps/` (否则 R2 那跳 404, 白白退化到公共代理)。
const PWSH_VER: &str = "7.4.6";
const NODE_VER: &str = "20.18.1";
const UV_VER: &str = "0.11.29";
/// Claude Code 版本 —— **直接抓 npm 平台包 (`@anthropic-ai/claude-code-<plat>`) 的 tgz 装**,
/// 不再走 `npm i -g`(它要下同一个 ~80MB 的 optional 依赖, 却常在解压/链接阶段卡死或半途而废,
/// 这是用户主诉的「装不下来」根因)。改成「一个 HTTP 下载 + tar 解压」后, 就能复用与 uv/Node
/// 同一套「R2 优先、多源回落、魔数校验」的下载机制。
/// 与 uv 同理: 锁版本才能把包镜像进 R2。升级 = 改这里 + 把三平台 tgz 传进 R2 的 `deps/`
/// (没传则 R2 那跳 404, 自动落到 npmmirror, 仍装得上)。
const CLAUDE_VER: &str = "2.1.218";

/// 把脚本正文里的占位符换成真值。用占位符而非 `format!` 的 `{}` —— PowerShell 脚本里全是
/// `${...}`/`{0:N0}` 这类花括号, 走 format! 得把每个都转义成 `{{}}`, 既难读又极易写错。
fn fill(script: &str) -> String {
    script
        .replace("DEPS_BASE", DEPS_BASE)
        .replace("MSI_MAGIC", MSI_MAGIC)
        .replace("ZIP_MAGIC", ZIP_MAGIC)
        .replace("GZ_MAGIC", GZ_MAGIC)
        .replace("CLAUDE_VER", CLAUDE_VER)
        .replace("PWSH_VER", PWSH_VER)
        .replace("NODE_VER", NODE_VER)
        .replace("UV_VER", UV_VER)
}

/// Node.js LTS 安装脚本 (前奏 + msiexec 封装 + 正文)。
#[cfg(any(windows, test))]
fn node_install_script() -> String {
    fill(&format!(
        "{PS_DOWNLOAD_PRELUDE}\n{PS_MSIEXEC_HELPER}\n{NODE_INSTALL_BODY}"
    ))
}

/// **便携版** Node.js 安装脚本 (前奏 + 正文) —— 后台静默安装专用, 不弹 UAC。
#[cfg(any(windows, test))]
pub(crate) fn node_portable_script() -> String {
    fill(&format!("{PS_DOWNLOAD_PRELUDE}\n{WIN_NODE_PORTABLE_BODY}"))
}

/// Windows **免管理员**的 Node.js 安装: 下载官方 zip 解压到 `~/.local/polaris-node`。
///
/// 为什么不复用上面那套 MSI/winget: 它们都要写 `Program Files` → `Start-Process -Verb RunAs`
/// **必弹 UAC**。而后台静默安装的前提就是「用户不用点任何东西」, 弹一个没有上下文的
/// 管理员授权框是最糟的体验(用户根本不知道这是谁弹的)。zip 解压到用户目录全程无提权,
/// npm 的全局前缀也落在同一个用户目录里, 装 claude 同样不需要管理员。
#[cfg(any(windows, test))]
const WIN_NODE_PORTABLE_BODY: &str = include_str!("install/win_node_portable_body.ps1");

/// PowerShell 7 安装脚本 (前奏 + msiexec 封装 + 正文)。
fn pwsh_install_script() -> String {
    fill(&format!(
        "{PS_DOWNLOAD_PRELUDE}\n{PS_MSIEXEC_HELPER}\n{PWSH_INSTALL_BODY}"
    ))
}

/// uv 安装脚本 (前奏 + 正文; uv 是绿色单文件, 不走 msiexec)。
#[cfg(windows)]
fn uv_install_script() -> String {
    fill(&format!("{PS_DOWNLOAD_PRELUDE}\n{UV_INSTALL_BODY}"))
}

/// macOS uv 安装脚本 (POSIX sh)。
#[cfg(target_os = "macos")]
fn mac_uv_install_script() -> String {
    fill(MAC_UV_INSTALL_SCRIPT)
}

/// Node.js LTS 安装脚本正文: winget 优先, 失败则下载官方 MSI (国内 npmmirror 镜像加速) 静默安装。
/// 选 20.x LTS ("Iron"): 长期支持、兼容 Windows 10。
#[cfg(any(windows, test))]
const NODE_INSTALL_BODY: &str = include_str!("install/node_install_body.ps1");

/// 安装 PowerShell 7。成功无需改 PATH (MSI / winget 安装都会自带配 PATH)。
///
/// 之前只用 `winget`, 但很多机器上要么没有 winget、要么 winget 源在国内拉不动
/// → 用户报「PowerShell 7 下载不了」。这里改成**两层策略**:
/// ① 有 winget 先用 winget (官方、能拿最新版);
/// ② winget 缺失 / 失败 → **直接下载官方 MSI 再 msiexec 静默安装**, 且下载走
///    国内可达的 GitHub 文件代理 (gh-proxy / ghfast) 兜底, 实在不行再走 GitHub 直连。
///    这就是「下载路径」修复 —— 明确把 MSI 落到 `%TEMP%` 再装, 不再黑盒依赖 winget。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn env_install_pwsh(app: AppHandle) -> Result<String, String> {
    if !cfg!(windows) {
        return Err("PowerShell 7 自动安装仅支持 Windows。".into());
    }
    let req_id = next_req_id();
    let cmd = build_powershell(&pwsh_install_script());
    stream_install(app, req_id.clone(), cmd, false, "PowerShell 7");
    Ok(req_id)
}

/// PowerShell 7 安装脚本正文: winget 优先, 失败则下载 MSI 静默安装。
/// 版本仅用于 MSI 兜底直链 (winget 路径自动取最新); 选 7.4.x LTS, 稳定且长期可用。
const PWSH_INSTALL_BODY: &str = include_str!("install/pwsh_install_body.ps1");

// ───────────────────────── uv (Python 运行时托管) 安装 ─────────────────────────

/// 安装 uv —— Python 脚本运行时的统一托管者。装到用户目录 `~/.local/bin`(免管理员/sudo),
/// 顺带写一份「国内镜像」uv 配置(仅当用户尚无配置时), 让之后 `uv python install` / 依赖解析走
/// 清华 PyPI + gh-proxy 的 python-build-standalone, 国内可达。装完无需改注册表: `env_check` 与
/// 启动期 `prime_path_for_claude` 都会把 `~/.local/bin` 并进进程 PATH, claude 当轮即可 `uv run`。
///
/// 下载走 GitHub Release 直链 + 国内文件代理(gh-proxy / ghfast)兜底(与 PowerShell 7 同策略),
/// 解压即用(uv 是单文件二进制, 无安装器、无 UAC)。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn env_install_uv(app: AppHandle) -> Result<String, String> {
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = &app;
        return Err(
            "uv 自动安装目前支持 Windows 与 macOS; Linux 请用 `curl -LsSf https://astral.sh/uv/install.sh | sh`。"
                .into(),
        );
    }
    #[cfg(any(windows, target_os = "macos"))]
    {
        let req_id = next_req_id();
        #[cfg(windows)]
        let cmd = build_powershell(&uv_install_script());
        #[cfg(target_os = "macos")]
        let cmd = build_install_shell(&mac_uv_install_script());
        stream_install(app, req_id.clone(), cmd, false, "uv");
        Ok(req_id)
    }
}

/// Windows uv 安装脚本正文: 下载 release zip → 解压到 `~/.local/bin` → 写国内镜像配置。
/// uv 是 MIT/Apache 双许可的单文件二进制, 解压即用, 不需要管理员权限。
///
/// **版本从 `latest/download` 改成锁定**: 要把包镜像进自家 R2 就必须锁版本 (R2 里放的是某个
/// 确定版本的字节)。副作用是升 uv 得改这里的版本号并重传 R2 —— 换来的是不再被公共代理拖累。
#[cfg(windows)]
const UV_INSTALL_BODY: &str = include_str!("install/uv_install_body.ps1");

/// macOS uv 安装脚本 (POSIX sh): 下载 release tar.gz(国内代理加速)→ 解压到 `~/.local/bin` →
/// 写国内镜像配置 + 写 PATH 进 shell 配置(与 Node 安装一致)。免 sudo。
#[cfg(target_os = "macos")]
const MAC_UV_INSTALL_SCRIPT: &str = include_str!("install/mac_uv_install_script.sh");

#[cfg_attr(feature = "desktop", tauri::command)]
pub fn env_cancel(req_id: String) -> Result<(), String> {
    if let Some(mut child) = CHILDREN.remove(&req_id) {
        let _ = child.kill();
    }
    Ok(())
}

// ───────────────────────── 内部: 流式安装 ─────────────────────────

/// 构造一个跑给定内联命令的系统 shell 进程:
/// - Windows → PowerShell (见 `build_powershell`);
/// - 类 Unix(含 macOS) → `sh -lc`(`-l` 走登录配置以拿到用户 PATH, npm 全局 bin 才在内)。
/// 安装/更新 Claude Code 这类跨平台命令统一走它。
pub(crate) fn build_install_shell(inner: &str) -> Command {
    #[cfg(windows)]
    {
        build_powershell(inner)
    }
    #[cfg(not(windows))]
    {
        let mut cmd = Command::new("sh");
        cmd.args(["-lc", inner]);
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        cmd
    }
}

/// 官方原生脚本 (境外网络兜底): Windows `install.ps1` / macOS·Linux `install.sh`。
/// 它从 claude.ai/GCS 拉二进制, **国内常被墙 → 仅给能访问外网的人**。
fn claude_native_cmd() -> String {
    #[cfg(windows)]
    {
        "irm https://claude.ai/install.ps1 | iex".to_string()
    }
    #[cfg(not(windows))]
    {
        "curl -fsSL https://claude.ai/install.sh | bash".to_string()
    }
}

/// Windows **免管理员**装 Claude Code: 直接抓 npm 平台包 tgz, 解压出 `claude.exe` 到
/// `~/.local/bin`(env_check 的首选探测位, 且已在启动期并进 PATH)。**不经 `npm i -g`** ——
/// 后者要下同一个 ~80MB 的 optional 依赖却常卡死(用户主诉), 这里退回最不会坏的「一个 HTTP
/// 下载 + tar 解压」形态, 并复用 R2 优先、多源回落、魔数校验的下载机制。
///
/// 源顺序: 自家 R2 打头(不开梯子最快) → npmmirror(阿里, 国内 CDN) → npmjs 官方。
/// 平台包内结构固定为 `package/claude.exe`(2026-07-24 实测), 故 `tar --strip-components=1`。
/// `tar.exe`(bsdtar) 是 Windows 10 1803+ 自带(最低支持 1809), 无需另装。
#[cfg(windows)]
const WIN_CLAUDE_PORTABLE_BODY: &str = include_str!("install/win_claude_portable_body.ps1");

/// Claude Code 直装脚本 (前奏 + 正文) —— 手动按钮与后台静默托管共用。
#[cfg(windows)]
fn claude_install_script() -> String {
    fill(&format!("{PS_DOWNLOAD_PRELUDE}\n{WIN_CLAUDE_PORTABLE_BODY}"))
}

/// 类 Unix (macOS / Linux) 直装 Claude Code (POSIX sh): 抓 npm 平台包 tgz → 解压
/// `package/claude` 到 `~/.local/bin`。绕开 `npm i -g` 的卡死, 走 R2 优先多源。免 sudo。
///
/// 平台判定覆盖 npm 上全部六个类 Unix 平台包 (2026-07-24 实测均存在):
/// `darwin-arm64` / `darwin-x64` / `linux-x64` / `linux-arm64` / `linux-x64-musl` /
/// `linux-arm64-musl`。**musl 与 glibc 是两个不同的包**, Alpine 之类拿错会跑不起来, 故按
/// `ldd --version` 判。包内结构 `package/claude` 与 Windows 的 `package/claude.exe`
/// 同一套 npm 打包约定 (三平台均已实测)。
#[cfg(not(windows))]
const UNIX_CLAUDE_INSTALL_SCRIPT: &str = include_str!("install/unix_claude_install_script.sh");

/// 默认方式安装 Claude Code 的进程命令 (直抓平台包 tgz, R2 优先多源, 不经 npm)。
/// 供手动安装按钮与后台静默托管 (`autopilot`) 共用, 两端一致。
pub(crate) fn claude_install_command() -> Command {
    #[cfg(windows)]
    {
        build_powershell(&claude_install_script())
    }
    // macOS 与 Linux 共用同一套 POSIX sh 脚本 (脚本内按 uname 自判平台包)。
    // Linux 以前留的是 `npm i -g` 老路 —— 那正是「下 80MB optional 依赖卡死」的根因,
    // 一并换掉; Docker 镜像本就预烤 claude(见 Dockerfile), 不经这里。
    #[cfg(not(windows))]
    {
        build_install_shell(&fill(UNIX_CLAUDE_INSTALL_SCRIPT))
    }
}

/// macOS 免 sudo 装 Node 的命令 (供后台静默托管复用)。
#[cfg(target_os = "macos")]
pub(crate) fn mac_node_install_command() -> Command {
    build_install_shell(MAC_NODE_INSTALL_SCRIPT)
}

/// 构造一个跑给定内联命令的 PowerShell 进程 (Bypass 执行策略, 以便 iex 远程脚本)。
fn build_powershell(inner: &str) -> Command {
    let mut cmd = Command::new("powershell");
    cmd.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        inner,
    ]);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    no_window(&mut cmd);
    cmd
}

#[path = "install/stream.rs"]
mod stream;
pub(crate) use stream::stream_install;

#[cfg(test)]
#[path = "install/tests.rs"]
mod tests;

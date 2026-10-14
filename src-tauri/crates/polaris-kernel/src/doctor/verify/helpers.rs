use super::*;

/// 版本探测类子命令的墙钟上限 (与 probe.rs 的探测一致)。
pub(super) const PROBE_TIMEOUT: Duration = Duration::from_secs(25);
/// 端到端冒烟的墙钟上限 —— 真发一次请求要过网络, 给足但必须有顶。
pub(super) const SMOKE_TIMEOUT: Duration = Duration::from_secs(120);
/// 冒烟的固定参数。与 chat 真正 spawn 时同源 (`--print` + bypass 权限), 保证「这里能过 =
/// 对话里也能过」; 但**工具集只给 Read** —— 冒烟跑在用户真机上, 物理上不给写的能力。
/// 抽成常量是为了让单测能直接盯住它 (见 `smoke_args_are_read_only`), 防有人图省事放开权限。
pub(super) const SMOKE_ARGS: [&str; 6] = [
    "--print",
    "--output-format",
    "text",
    "--permission-mode=bypassPermissions",
    "--allowedTools",
    "Read",
];

// ───────────────────────── 小工具 ─────────────────────────

pub(super) fn ms_since(t: Instant) -> u64 {
    t.elapsed().as_millis() as u64
}

/// 截断多行输出, 只留前几行 —— 面板里展示用, 不是日志窗口。
pub(super) fn brief(s: &str, max_lines: usize, max_chars: usize) -> Option<String> {
    let joined = s
        .lines()
        .map(|l| l.trim_end())
        .filter(|l| !l.trim().is_empty())
        .take(max_lines)
        .collect::<Vec<_>>()
        .join("\n");
    if joined.is_empty() {
        return None;
    }
    Some(if joined.chars().count() > max_chars {
        let cut: String = joined.chars().take(max_chars).collect();
        format!("{cut}…")
    } else {
        joined
    })
}

/// 敏感值掩码 —— 只留头 4 尾 4。**绝不回传明文**: 这份报告会经数据面走到手机端。
pub(super) fn mask(v: &str) -> String {
    let n = v.chars().count();
    if n <= 8 {
        return "••••".to_string();
    }
    let head: String = v.chars().take(4).collect();
    let tail: String = v.chars().skip(n - 4).collect();
    format!("{head}••••{tail}")
}

pub(super) fn out_text(out: &std::process::Output) -> String {
    let mut s = String::from_utf8_lossy(&out.stdout).to_string();
    let e = String::from_utf8_lossy(&out.stderr);
    if !e.trim().is_empty() {
        if !s.trim().is_empty() {
            s.push('\n');
        }
        s.push_str(&e);
    }
    s
}

pub(super) fn first_nonempty(s: &str) -> Option<String> {
    s.lines()
        .map(|l| l.trim())
        .find(|l| !l.is_empty())
        .map(|s| s.to_string())
}

/// 路径归一 (小写 + 去尾斜杠), 用于去重 / 比对。
pub(super) fn norm(p: &Path) -> String {
    p.to_string_lossy()
        .trim_end_matches(['\\', '/'])
        .to_lowercase()
}

/// 由一个 claude 可执行文件推出它所属的「安装根」——
/// npm 装的 `<prefix>/node_modules/@anthropic-ai/claude-code/bin/claude.exe` 与
/// `<prefix>/claude.cmd` 是**同一份安装的两个入口**, 必须归成一类, 否则一份安装会被
/// 误报成两份冲突。其余情况取父目录。
pub(super) fn install_root(p: &Path) -> PathBuf {
    let comps: Vec<_> = p.components().collect();
    if let Some(i) = comps.iter().position(|c| c.as_os_str() == "node_modules") {
        return comps[..i].iter().collect();
    }
    p.parent().map(|x| x.to_path_buf()).unwrap_or_default()
}

/// 给安装根起个人话名字 (面板里比一长串路径好认)。
pub(super) fn root_label(root: &Path) -> String {
    let n = norm(root);
    let home = home_dir().map(|h| norm(&h)).unwrap_or_default();
    if !home.is_empty() {
        if n == format!("{home}\\.local\\bin") || n == format!("{home}/.local/bin") {
            return "官方脚本 / 直装 (~/.local/bin)".into();
        }
        if n.contains("polaris-node") {
            return "应用内置便携 Node 的全局安装".into();
        }
    }
    if npm_global_prefix().map(|p| norm(&p)) == Some(n.clone()) {
        return "npm 全局安装".into();
    }
    if n.contains("windowsapps") {
        return "Microsoft Store 执行别名".into();
    }
    to_fwd(root)
}

/// 挑一个「能直接跑」的入口来探版本: 优先 `.exe`, 再 `.cmd`, 否则第一个。
pub(super) fn best_entry(paths: &[PathBuf]) -> Option<PathBuf> {
    let rank = |p: &Path| -> u8 {
        match p
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
        {
            Some(e) if e == "exe" => 0,
            Some(e) if e == "cmd" || e == "bat" => 1,
            None => {
                if cfg!(windows) {
                    2
                } else {
                    0
                }
            }
            _ => 2,
        }
    };
    let mut v = paths.to_vec();
    v.sort_by_key(|p| rank(p));
    v.into_iter().next()
}


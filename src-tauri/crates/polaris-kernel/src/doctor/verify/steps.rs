use super::*;

// ───────────────────────── 「新开终端」语义的探测 ─────────────────────────

/// 在**新开终端的环境**里探到的东西。
pub(super) struct TerminalProbe {
    /// 终端里 `where claude` / `command -v claude` 的命中 —— **首个即终端实际会跑的那份**
    pub(super) hits: Vec<PathBuf>,
    /// 终端里 `claude --version` 的输出首行 (跑起来了才有)
    pub(super) version: Option<String>,
    /// 展示用命令行
    pub(super) cmdline: String,
    /// 原始输出 (失败时给用户看)
    pub(super) raw: String,
}

/// 复现「用户刚新开一个终端」的 PATH。
///
/// Windows: 新终端的 PATH = **注册表里的机器级 + 用户级**, 与本进程 PATH 无关 ——
/// 本进程那份早被 `prime_path_for_claude` 前插过 claude / node / pwsh 目录, 拿它去测
/// 「终端里能不能用」等于自问自答。读不到注册表 (极少见) 才退回进程 PATH。
#[cfg(windows)]
pub(super) fn fresh_terminal_path() -> Option<String> {
    let machine = read_machine_path().unwrap_or_default();
    let user = read_user_path().unwrap_or_default();
    let parts: Vec<&str> = [machine.trim(), user.trim()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() {
        return None;
    }
    Some(parts.join(";"))
}

/// 类 Unix: 新终端的 PATH 由登录 shell 读 profile 现算。这里先把 PATH 压回系统最小集
/// (= GUI 进程从 Finder 拿到的那份), 再交给登录 shell 自己拼 —— 应用启动时并进进程 PATH
/// 的那些目录不会漏进来干扰判断。
#[cfg(not(windows))]
pub(super) fn minimal_unix_path() -> &'static str {
    "/usr/bin:/bin:/usr/sbin:/sbin"
}

/// 给「模拟终端」的子进程套上干净环境: 覆写 PATH, 并摘掉应用自己注入的变量
/// (`CLAUDE_CODE_GIT_BASH_PATH` 是启动预热设的, 真终端里没有)。
pub(super) fn terminal_env(cmd: &mut Command, path: Option<&str>) {
    if let Some(p) = path {
        cmd.env("PATH", p);
    }
    cmd.env_remove("CLAUDE_CODE_GIT_BASH_PATH");
    cmd.stdin(Stdio::null());
    no_window(cmd);
}

#[cfg(windows)]
pub(super) fn terminal_probe() -> TerminalProbe {
    let path = fresh_terminal_path();
    let mk = |args: &[&str]| {
        let mut c = Command::new("cmd");
        c.arg("/d").arg("/c").args(args);
        terminal_env(&mut c, path.as_deref());
        c
    };
    // ① 终端会命中哪些 claude (where.exe 的顺序 = cmd 解析裸名的顺序)
    let hits = output_with_timeout(mk(&["where", "claude"]), PROBE_TIMEOUT)
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(|l| l.trim())
                .filter(|l| !l.is_empty())
                .map(PathBuf::from)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    // ② 真的敲一次 `claude --version`
    let out = output_with_timeout(mk(&["claude", "--version"]), PROBE_TIMEOUT);
    let raw = out.as_ref().map(out_text).unwrap_or_default();
    let version = out
        .as_ref()
        .filter(|o| o.status.success())
        .and_then(|o| first_nonempty(&String::from_utf8_lossy(&o.stdout)));
    TerminalProbe {
        hits,
        version,
        cmdline: "cmd /c claude --version   (PATH = 注册表 机器级 + 用户级)".into(),
        raw,
    }
}

#[cfg(not(windows))]
pub(super) fn terminal_probe() -> TerminalProbe {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let base = minimal_unix_path();
    let mk = |script: &str| {
        let mut c = Command::new(&shell);
        c.args(["-lc", script]);
        terminal_env(&mut c, Some(base));
        c
    };
    let hits = output_with_timeout(mk("command -v claude"), PROBE_TIMEOUT)
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(|l| l.trim())
                .filter(|l| !l.is_empty())
                .map(PathBuf::from)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let out = output_with_timeout(mk("claude --version"), PROBE_TIMEOUT);
    let raw = out.as_ref().map(out_text).unwrap_or_default();
    let version = out
        .as_ref()
        .filter(|o| o.status.success())
        .and_then(|o| first_nonempty(&String::from_utf8_lossy(&o.stdout)));
    TerminalProbe {
        hits,
        version,
        cmdline: format!("{shell} -lc 'claude --version'   (PATH 压回系统最小集, 由登录 shell 自行加载)"),
        raw,
    }
}

// ───────────────────────── 各步实跑 ─────────────────────────

/// ① 应用内实跑: 用 `resolve_claude_exe` 解析出的**绝对路径**执行 —— 与 chat 真正 spawn
/// claude 时走的是同一条解析路径, 所以这一步绿 = 「在应用里发消息能跑起来」。
pub(super) fn step_exec(exe: Option<&Path>) -> (bool, VerifyStep) {
    let t = Instant::now();
    let Some(exe) = exe else {
        return (
            false,
            VerifyStep {
                key: "exec".into(),
                name: "应用内可执行".into(),
                status: "fail".into(),
                detail: "没解析出任何 claude 可执行文件 —— 尚未安装, 或安装位置不在已知候选里。"
                    .into(),
                command: None,
                output: None,
                ms: ms_since(t),
            },
        );
    };
    let out = output_with_timeout(command_at(exe, &["--version"]), PROBE_TIMEOUT);
    let ok = out.as_ref().is_some_and(|o| o.status.success());
    let ver = out
        .as_ref()
        .and_then(|o| first_nonempty(&String::from_utf8_lossy(&o.stdout)));
    let raw = out.as_ref().map(out_text).unwrap_or_default();
    let detail = match (ok, &ver) {
        (true, Some(v)) => format!("跑起来了: {v}"),
        (true, None) => "进程正常退出, 但没有版本输出 (claude 输出格式可能变了)。".into(),
        (false, _) if raw.trim().is_empty() => {
            "起不来 —— 进程没有任何输出 (超时 / 被杀 / 文件损坏)。可尝试重新安装。".into()
        }
        (false, _) => "起不来 —— 文件存在但执行失败, 见下方输出。可尝试重新安装。".into(),
    };
    (
        ok,
        VerifyStep {
            key: "exec".into(),
            name: "应用内可执行".into(),
            status: if ok { "ok" } else { "fail" }.into(),
            detail,
            command: Some(format!("{} --version", to_fwd(exe))),
            output: brief(&raw, 6, 400),
            ms: ms_since(t),
        },
    )
}

/// ② 终端实跑 —— 「能不能在电脑终端里用」。
pub(super) fn step_terminal(term: &TerminalProbe, app_runnable: bool, t: Instant) -> VerifyStep {
    let ok = term.version.is_some();
    let detail = match (ok, app_runnable) {
        (true, _) => {
            let v = term.version.clone().unwrap_or_default();
            let which = term
                .hits
                .first()
                .map(|p| format!("  (命中 {})", to_fwd(p)))
                .unwrap_or_default();
            format!("终端里敲 `claude` 能跑: {v}{which}")
        }
        (false, true) => "**应用内能跑, 但新开的终端里跑不起来** —— 典型是 claude 所在目录只进了\
             本进程 PATH, 没写进用户 PATH。点上方「修复 PATH」后新开终端即可。"
            .into(),
        (false, false) => "终端里也跑不起来 —— claude 尚未安装 / 安装损坏, 先把上一项修好。".into(),
    };
    VerifyStep {
        key: "terminal".into(),
        name: "电脑终端里可运行".into(),
        status: if ok { "ok" } else { "fail" }.into(),
        detail,
        command: Some(term.cmdline.clone()),
        output: (!ok).then(|| brief(&term.raw, 6, 400)).flatten(),
        ms: ms_since(t),
    }
}

/// ③ claude 依赖的 shell 实跑 —— claude 所有命令类工具都靠它。Windows 上必须是**真身**
/// pwsh 或 Git Bash (Store 别名在无控制台的子进程里起不来, 见 `is_app_exec_alias`)。
pub(super) fn step_shell() -> VerifyStep {
    let t = Instant::now();
    #[cfg(windows)]
    let (exe, args, label): (Option<PathBuf>, Vec<&str>, &str) = {
        if let Some(bash) = git_bash_path() {
            (Some(bash), vec!["-c", "echo polaris-shell-ok"], "Git Bash")
        } else if let Some(p) = pwsh_candidates().into_iter().find(|p| p.exists()) {
            (
                Some(p),
                vec!["-NoProfile", "-NonInteractive", "-Command", "'polaris-shell-ok'"],
                "PowerShell 7",
            )
        } else {
            (None, vec![], "")
        }
    };
    #[cfg(not(windows))]
    let (exe, args, label): (Option<PathBuf>, Vec<&str>, &str) = (
        Some(PathBuf::from("/bin/sh")),
        vec!["-c", "echo polaris-shell-ok"],
        "系统 sh",
    );

    let Some(exe) = exe else {
        return VerifyStep {
            key: "shell".into(),
            name: "Claude 可用的 Shell".into(),
            status: "fail".into(),
            detail: "找不到可用 shell (真身 PowerShell 7 / Git Bash) —— claude 里的命令类工具会全部报错。\
                 正常情况下应用已内置 Git Bash, 出现此项说明内置资源缺失, 建议重装应用或装 PowerShell 7。"
                .into(),
            command: None,
            output: None,
            ms: ms_since(t),
        };
    };
    let out = output_with_timeout(command_at(&exe, &args), PROBE_TIMEOUT);
    let raw = out.as_ref().map(out_text).unwrap_or_default();
    let ok = out.as_ref().is_some_and(|o| o.status.success()) && raw.contains("polaris-shell-ok");
    let bundled = crate::doctor::bundled::is_bundled(&exe);
    VerifyStep {
        key: "shell".into(),
        name: "Claude 可用的 Shell".into(),
        status: if ok { "ok" } else { "fail" }.into(),
        detail: if ok {
            format!(
                "{label} 实跑通过{}。",
                if bundled { " (随应用内置)" } else { "" }
            )
        } else {
            format!("{label} 起不来 —— claude 里执行命令的工具会失败。")
        },
        command: Some(format!("{} -c echo …", to_fwd(&exe))),
        output: (!ok).then(|| brief(&raw, 4, 300)).flatten(),
        ms: ms_since(t),
    }
}

/// claude 的配置文件里哪些是坏的 (存在但不是合法 JSON) —— 半截 JSON 会让 claude 每次启动即崩,
/// 而 `--version` 有时还能过, 所以必须单独查。返回 (路径, 错因)。
pub(super) fn broken_config_files() -> Vec<(String, String)> {
    let Some(home) = home_dir() else {
        return Vec::new();
    };
    [
        home.join(".claude.json"),
        home.join(".claude").join("settings.json"),
        home.join(".claude").join("settings.local.json"),
    ]
    .into_iter()
    .filter(|p| p.is_file())
    .filter_map(|p| {
        let txt = std::fs::read_to_string(&p).ok()?;
        if txt.trim().is_empty() {
            return Some((to_fwd(&p), "文件是空的".to_string()));
        }
        match serde_json::from_str::<serde_json::Value>(&txt) {
            Ok(_) => None,
            Err(e) => Some((to_fwd(&p), e.to_string())),
        }
    })
    .collect()
}

/// ④ 配置文件完好性。
pub(super) fn step_config(broken: &[(String, String)]) -> VerifyStep {
    let t = Instant::now();
    if broken.is_empty() {
        return VerifyStep {
            key: "config".into(),
            name: "Claude 配置文件".into(),
            status: "ok".into(),
            detail: "~/.claude.json 与 ~/.claude/settings*.json 均可正常解析 (或尚未生成)。".into(),
            command: None,
            output: None,
            ms: ms_since(t),
        };
    }
    VerifyStep {
        key: "config".into(),
        name: "Claude 配置文件".into(),
        status: "fail".into(),
        detail: format!(
            "有 {} 个配置文件不是合法 JSON —— claude 启动时会直接报错退出。备份后删除损坏的那个即可 (claude 会重建)。",
            broken.len()
        ),
        command: None,
        output: brief(
            &broken
                .iter()
                .map(|(p, e)| format!("{p}: {e}"))
                .collect::<Vec<_>>()
                .join("\n"),
            4,
            400,
        ),
        ms: ms_since(t),
    }
}

/// ⑤ 认证就绪度。**只看有没有凭据, 不回传任何明文**。
/// 注意这一步永远不判 fail: 官方登录在 macOS 上可能存在 keychain 里、查不到文件也照样能用,
/// 真要确证得走深度冒烟 (step_smoke)。
pub(super) fn step_auth() -> VerifyStep {
    let t = Instant::now();
    let injected: Vec<&str> = ["ANTHROPIC_AUTH_TOKEN", "ANTHROPIC_API_KEY"]
        .into_iter()
        .filter(|k| {
            std::env::var(k)
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false)
        })
        .collect();
    let base = std::env::var("ANTHROPIC_BASE_URL").unwrap_or_default();
    let creds = home_dir()
        .map(|h| h.join(".claude").join(".credentials.json"))
        .filter(|p| p.is_file());

    let (status, detail) = if !injected.is_empty() {
        (
            "ok",
            format!(
                "应用会为 claude 注入供应商凭据 ({}){}。",
                injected.join(" / "),
                if base.trim().is_empty() {
                    String::new()
                } else {
                    format!("; 上游 {}", base.trim())
                }
            ),
        )
    } else if creds.is_some() {
        (
            "ok",
            "已用 Claude 官方账号登录 (~/.claude/.credentials.json)。".to_string(),
        )
    } else {
        (
            "warn",
            "没查到可用凭据 —— 若还没在「供应商」页配 API、也没用官方账号登录, claude 起得来但发不出请求。\
             (macOS 官方登录可能存在钥匙串里, 查不到文件不代表没登录, 可用下方「深度检测」确证。)"
                .to_string(),
        )
    };
    VerifyStep {
        key: "auth".into(),
        name: "凭据就绪".into(),
        status: status.into(),
        detail,
        command: None,
        output: None,
        ms: ms_since(t),
    }
}

/// ⑥ 端到端冒烟 —— **真发一次最小请求**。这是「真正可用」的唯一硬证据: 前面几步全绿也可能
/// 卡在没登录 / key 失效 / 上游不通 / 被墙。只在 deep=true 时跑 (会消耗一点点额度)。
///
/// 参数与 chat 真正 spawn 时同源 (`--print` + 只读工具 + bypass 权限), 保证「这里能过 =
/// 对话里也能过」; 工作目录用临时目录, 不碰用户项目。
pub(super) fn step_smoke(exe: Option<&Path>, app_runnable: bool) -> VerifyStep {
    let t = Instant::now();
    if !app_runnable {
        return VerifyStep {
            key: "smoke".into(),
            name: "端到端冒烟 (真发一次请求)".into(),
            status: "skip".into(),
            detail: "claude 都起不来, 冒烟跳过。".into(),
            command: None,
            output: None,
            ms: ms_since(t),
        };
    }
    let exe = exe.expect("app_runnable 为真时必有解析出的路径");
    let mut cmd = command_at(exe, &SMOKE_ARGS);
    cmd.current_dir(std::env::temp_dir())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    harden_child_env(&mut cmd); // loopback NO_PROXY + 内置运行时 + 清干扰变量
    crate::provider::scope_child_claude(&mut cmd); // 跟对话用同一家供应商

    // 提示词故意极短: 冒烟只为证明「这条链路通」, 不为产出内容。
    let prompt = "只回复两个字符: OK";
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return VerifyStep {
                key: "smoke".into(),
                name: "端到端冒烟 (真发一次请求)".into(),
                status: "fail".into(),
                detail: format!("调起 claude 失败: {e}"),
                command: Some(format!("{} --print …", to_fwd(exe))),
                output: None,
                ms: ms_since(t),
            }
        }
    };
    if let Some(mut si) = child.stdin.take() {
        use std::io::Write as _;
        let _ = si.write_all(prompt.as_bytes());
    } // drop → 关 stdin, claude 才知道输入结束

    // 墙钟看门狗: 到点 kill, 免得没配好凭据时挂死在这里。
    let out = wait_with_timeout(child, SMOKE_TIMEOUT);
    let (ok, raw) = match &out {
        Some(o) => (o.status.success(), out_text(o)),
        None => (false, format!("超时 {}s, 已终止。", SMOKE_TIMEOUT.as_secs())),
    };
    let body = out
        .as_ref()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    let real_ok = ok && !body.is_empty();
    VerifyStep {
        key: "smoke".into(),
        name: "端到端冒烟 (真发一次请求)".into(),
        status: if real_ok { "ok" } else { "fail" }.into(),
        detail: if real_ok {
            format!("链路通 —— claude 真的回话了: {}", brief(&body, 1, 80).unwrap_or_default())
        } else if ok {
            "claude 正常退出但没有任何回复 —— 多半是没登录 / 未配 API key。".into()
        } else {
            "发请求失败 —— 见下方输出 (常见: 未登录、key 失效、上游不通或被代理挡住)。".into()
        },
        command: Some(format!("{} --print \"{prompt}\"", to_fwd(exe))),
        output: brief(&raw, 8, 600),
        ms: ms_since(t),
    }
}

/// 等一个已 spawn 的子进程, 超时则 kill 并回 None。stdout/stderr 各由独立线程读到 EOF,
/// 避免管道写满反压自锁 (同 `output_with_timeout` 的做法, 这里因需要先喂 stdin 才分开写)。
pub(super) fn wait_with_timeout(mut child: std::process::Child, timeout: Duration) -> Option<std::process::Output> {
    let mut op = child.stdout.take()?;
    let mut ep = child.stderr.take()?;
    let (tx_o, rx_o) = std::sync::mpsc::channel();
    let (tx_e, rx_e) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = std::io::Read::read_to_end(&mut op, &mut b);
        let _ = tx_o.send(b);
    });
    std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = std::io::Read::read_to_end(&mut ep, &mut b);
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
                    return None;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_) => return None,
        }
    };
    Some(std::process::Output {
        status,
        stdout: rx_o.recv().unwrap_or_default(),
        stderr: rx_e.recv().unwrap_or_default(),
    })
}


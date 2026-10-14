use super::*;

// Docker-in-Docker 沙箱仅桌面构建可用 (依赖 polaris_sandbox crate)；
// server(容器内)本期降级，不编译此路径。
#[cfg(feature = "desktop")]
#[allow(dead_code)]
fn spawn_in_sandbox(prompt: &str, perm: &str) -> Result<Child, String> {
    let perm_flag = format!("--permission-mode={}", perm);
    // 联网 + (非只读档位)本地读写执行, 让成品能真正产出
    let allowed = allowed_tools_for(perm, false);
    // 沙箱内 KB 永远挂在 /kb (sandbox_start 时挂载),
    // 这里让 claude 把 /kb 也加进可读目录,并以 /workspace 为 cwd
    let mut cmd = Command::new("docker");
    cmd.args([
        "exec",
        "-i",
        "-w",
        "/workspace",
        // 与 polaris_sandbox::CONTAINER_NAME 同值。字面内联以斩断 kernel→sandbox 的
        // 向上依赖(红线 R3);本函数是 dead_code 保留路径,真启用时应经壳层桥接注入。
        "polaris-sandbox",
        "claude",
        "--print",
        "--output-format",
        "stream-json",
        "--verbose",
    ]);
    // token 级部分流(同 spawn_on_host);flag 必须在 prompt 位置参数之前。
    if partial_stream_enabled() {
        cmd.arg("--include-partial-messages");
    }
    cmd.args([
        "--add-dir",
        "/kb",
        "--allowedTools",
        &allowed,
        &perm_flag,
        prompt,
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    no_window(&mut cmd); // 隐藏式: 不弹控制台窗口
    let child = cmd
        .spawn()
        .map_err(|e| format!("在沙箱内调起 claude 失败: {}", e))?;
    Ok(child)
}

/// 本轮 claude CLI 的实际工作目录(cwd)。优先用项目绑定的工作目录(用户选定的真实文件夹,
/// 等同终端 `cd <dir> && claude`), 否则回落编译期仓库根(开发)/进程当前目录(装机)。
/// 抽出来让 spawn、extra_claude_dirs、常驻会话指纹三处对 cwd 的口径**完全一致** —— 否则
/// 指纹里算出的 add_dirs(相对 cwd 判「在不在子树」)会与真 spawn 的 cwd 不符。
/// work_dir 传进来时若已不是有效目录(用户在应用外删了)则忽略, 回落默认, spawn 不会拿无效 cwd 失败。
pub(crate) fn effective_cwd(work_dir: Option<&Path>) -> std::path::PathBuf {
    work_dir
        .filter(|p| p.is_dir())
        .map(Path::to_path_buf)
        .or_else(claude_md::project_root)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")))
}

/// 需要 --add-dir 显式放行的目录(KB 根/产物目录, 均可能不在 cwd 子树)。
/// 抽成独立函数: spawn 与常驻会话的「配置指纹」共用同一来源 —— 目录集合变了
/// (用户移动 KB / 换对话产物目录 / 换项目工作目录)意味着旧进程的可访问面已不对, 指纹判变触发重建。
/// cwd 由调用方传入(= effective_cwd 的结果): 项目绑了工作目录后 cwd 是用户项目仓库,
/// KB 根/产物目录几乎必然不在其子树 → 照常经 --add-dir 放行, claude 才读得到。
pub(crate) fn extra_claude_dirs(cwd: &Path, art_dir: &Path) -> Vec<String> {
    // 本源模式: 一个 --add-dir 都不传, 只信任 cwd 子树 —— 与终端裸调 claude 一致。
    // 闸设在这里(而非只在 spawn 侧)是刻意的: 本函数同时也是常驻会话指纹 add_dirs 字段的
    // 唯一来源, 两处口径必须一致, 否则指纹算的可访问面与真 spawn 的对不上。
    if raw_mode() {
        return Vec::new();
    }
    // 如果 KB root 不在 cwd 子树下(用户可能把 KB 移到别处), 用 --add-dir 显式放行
    let kb_root = std::path::PathBuf::from(
        crate::chat::bridges::kb_bridge()
            .map(|b| b.root())
            .unwrap_or_default(),
    );
    let mut dirs: Vec<String> = Vec::new();
    if !kb_root.as_os_str().is_empty() && kb_root.exists() && !kb_root.starts_with(cwd) {
        dirs.push(kb_root.to_string_lossy().to_string());
    }
    // 产物目录在 ~/Polaris 下, 不在 cwd 子树, 显式放行 claude 可写入
    if art_dir.exists() && !art_dir.starts_with(cwd) {
        dirs.push(art_dir.to_string_lossy().to_string());
    }
    dirs
}

/// 按模式取显式配置的 --model(未配置返回 None, 保持供应商钉死的模型)。
/// 抽出来供常驻会话指纹使用: 用户改了 POLARIS_WORK_MODEL/POLARIS_FAST_MODEL,
/// 旧常驻进程还钉着老模型, 必须判变重建。
pub(crate) fn model_for_mode(work_full: bool) -> Option<String> {
    let model_env = if work_full {
        "POLARIS_WORK_MODEL"
    } else {
        "POLARIS_FAST_MODEL"
    };
    std::env::var(model_env)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// 看门狗阈值 (idle_secs, hard_cap_secs): env 覆写语义与默认值不变(详注见
/// chat_send_pipeline 内看门狗一节); 抽出来让常驻路径每轮的看门狗吃同一套配置。
pub(crate) fn watchdog_config() -> (u64, u64) {
    // idle 判死是「疑似挂死」的入口, 真正杀不杀还要过进程树深检(有子孙/CPU 推进就豁免)。
    // 但深检盖不住一类**正常**静默: claude 本体在等上游(限流退避/慢 API/长响应首包)时
    // 零输出、零子孙、CPU 也几乎不动 —— 600s 阈值会把 7h 级长任务在等待窗口里误杀。
    // 放宽到 1800s(30min): 真挂死(网络吊死/无界扫描)迟早静止照样回收, 长等待不再中断。
    let idle = std::env::var("POLARIS_CHAT_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(if cfg!(feature = "desktop") { 1800 } else { 900 });
    // 硬顶 = 单轮总时长的**绝对**上限, 不受 idle 深检的「CPU 在推进」豁免影响
    // (HardTimeout 判据独立于 IdleKilled 分支)。此前桌面 7200s(2h)/容器 3600s(1h) ——
    // 一个持续产出的 7 小时长任务(批量创作/大编译/整夜跑脚本)到点被**无条件**杀掉,
    // 这是「长任务必中断」的根因。改为 86400s(24h): 正常长任务(含 7h 级)绝不会撞线,
    // 「假死但 CPU 空转」仍有必然终止的天花板, 不会永久霸占资源。
    // POLARIS_CHAT_HARD_CAP_SECS 仍可覆写(0=显式关掉)。
    let cap = std::env::var("POLARIS_CHAT_HARD_CAP_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(86400);
    (idle, cap)
}

pub(crate) fn spawn_on_host(
    prompt: &str,
    perm: &str,
    art_dir: &Path,
    with_task: bool,
    work_full: bool,
    provider_id: Option<&str>,
    persistent: bool,
    work_dir: Option<&Path>,
) -> Result<Child, String> {
    let perm_flag = format!("--permission-mode={}", perm);
    // cwd = 项目绑定的工作目录(用户选的真实仓库, 等同终端 `cd <dir> && claude`), 未绑则回落
    // 编译期 polaris-app 根 / 进程当前目录。claude CLI 自动信任整棵 cwd 子树。
    let cwd = effective_cwd(work_dir);

    // KB 根写一份 .ignore(幂等), 让 ripgrep(Grep 工具/rg)自动跳 output/二进制/大素材 → 检索更快
    ensure_kb_search_ignore();

    let mut args: Vec<String> = vec![
        "--print".into(),
        "--output-format".into(),
        "stream-json".into(),
        "--verbose".into(),
    ];
    // 常驻 agent 模式: stdin 从「一次性 prompt 通道」变成逐轮 stream-json 消息通道,
    // 进程收完一轮吐 result 事件后**不退出**, 阻塞等下一行 user 消息 —— 这是免掉
    // 每消息 ~6.4s CLI 自举的关键(见 session_pool 模块头注释)。
    if persistent {
        args.push("--input-format".into());
        args.push("stream-json".into());
    }
    // token 级部分流:CLI 吐 content_block_delta 逐字增量 → 前端豆包式逐字上屏。
    // 默认开;POLARIS_PARTIAL_STREAM=0 关(兼容旧版 CLI,见 partial_stream_enabled)。
    if partial_stream_enabled() {
        args.push("--include-partial-messages".into());
    }
    for d in extra_claude_dirs(&cwd, art_dir) {
        args.push("--add-dir".into());
        args.push(d);
    }
    // 本源模式: 不传任何工具白/黑名单 —— 工具面交回 CLI 自己按 --permission-mode 与用户
    // ~/.claude 配置决定, 与终端裸调一致(见 raw_mode 文档)。常规路径照旧两条都传。
    if !raw_mode() {
        // 联网工具默认放行; 非「拒绝授权」档位再叠加本地读写执行 (Bash/PowerShell/文件),
        // 否则 headless 下连 `python xxx.py` 都被拒, .pptx/.xlsx 这类成品根本产不出来。
        args.push("--allowedTools".into());
        args.push(allowed_tools_for(perm, with_task));
        // 快速模式: 弃用冗余工具(Task/NotebookEdit)。disallowedTools 优先级高于 allowedTools。
        if let Some(disallowed) = disallowed_tools_for(work_full, with_task) {
            args.push("--disallowedTools".into());
            args.push(disallowed);
        }
    }
    // 模型档跟随模式(可选, 默认不启用): 多模型供应商上可让快速模式走快档(便宜快)、工作模式走强档。
    // 仅当对应环境变量显式设了 model id 才传 --model —— 单模型网关/未配置时**保持原样**(供应商
    // 钉死的模型), 绝不因传错 model 名把请求打挂。POLARIS_WORK_MODEL / POLARIS_FAST_MODEL。
    if let Some(m) = model_for_mode(work_full) {
        args.push("--model".into());
        args.push(m);
    }
    args.push(perm_flag);
    // ⚠️ prompt 不再塞 argv 末尾 —— 走 stdin。
    // Windows CreateProcessW 的 lpCommandLine 上限 32767 字符, 你 KB 全文 + 多轮对话历史
    // 拼一起轻松爆, 直接抛 206 ERROR_FILENAME_TOO_LONG 拒 spawn (实测 33k 字符就 100% 复现)。
    // 改 stdin 后 prompt 长度无限制。kb.rs 的 spawn_in_sandbox 早就这么干了 (注释在那)。
    let _ = prompt; // 函数签名仍保留 prompt 参数, 调用方写 stdin

    // 解析 claude 可执行文件的全路径再 spawn, 而非裸名 "claude":
    // npm 装只在 PATH 放 `claude.cmd`, 而 Windows CreateProcessW 解析裸名只补 `.exe`、不查 PATHEXT
    // → 裸名找不到 npm 装的 claude。resolve_claude_exe 会挖出真·原生 exe (原生装 / npm 装通吃);
    // 解析不到再回退裸名靠 PATH (兼容用户自行配好的环境)。
    let claude_bin: std::ffi::OsString = crate::doctor::resolve_claude_exe()
        .map(|p| p.into_os_string())
        .unwrap_or_else(|| "claude".into());
    let mut cmd = Command::new(&claude_bin);
    cmd.args(&args)
        .current_dir(&cwd)
        .stdin(Stdio::piped()) // 接 prompt 用, 调用方 spawn 后 write + drop
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // 子进程环境净化: loopback 强制 NO_PROXY (切 Codex 时 claude 走 127.0.0.1 本地代理,
    // 系统代理会劫持回环 → 连不上) + 清 DEBUG/LD_PRELOAD。见 doctor::harden_child_env。
    // Windows 的 Git Bash (CLAUDE_CODE_GIT_BASH_PATH) 与随包运行时 (uv/Python/msys 工具目录)
    // 也一并由它注入 —— 这里曾另有一段 git-bash 兜底, 与之重复且判断更弱, 已并进单一入口。
    crate::doctor::harden_child_env(&mut cmd);
    // 隔离模式跑第三方 → CLAUDE_CONFIG_DIR 指私有目录, 会话账本不进 ~/.claude/projects,
    // cc-switch 等外部监控不再看见 Polaris 自动任务的第三方会话。
    // provider_id 指定了本对话这家 → 逐命令注入它的 env(真隔离, 与全局开关/其它对话解耦);
    // None/"auto" → 回落全局当前供应商(Auto 档)。统一走这一个入口。
    crate::provider::scope_child_claude_by_id(&mut cmd, provider_id);
    no_window(&mut cmd); // 隐藏式: 每次发消息不再弹出黑色终端窗口

    // Linux/容器: 让 claude 成为新进程组的组长 (setpgid)。这样 kill_tree 的
    // `kill -TERM -<pid>` 能一次带走 claude 扇出的 python/node/dev-server 整棵子孙树,
    // 不留孤儿占端口/CPU —— 对容器内长稳运行(>3h, 反复发消息)至关重要。
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    cmd.spawn().map_err(|e| {
        // 错误只在 spawn 本身失败 (e.g. exe 找不到), 不再是 prompt 太长。
        // Windows 上架构不符的 exe 会返回 os error 193 (ERROR_BAD_EXE_FORMAT ——
        // 系统原文「%1 不是有效的 Win32 应用程序」)。裸透传会在界面上冒出「Win32 不匹配」这种
        // 让人摸不着头脑的串, 这里翻成人话 + 修复指引 (多半是本机装的 claude 二进制架构与系统不符)。
        #[cfg(windows)]
        if e.raw_os_error() == Some(193) {
            let arch = std::env::consts::ARCH; // x86_64 / aarch64
            return format!(
                "调起宿主机 claude CLI 失败: 本机安装的 claude 二进制架构与系统 ({arch}) 不符 \
                 (Windows: 不是有效的 Win32 应用程序 / ERROR_BAD_EXE_FORMAT)。\
                 请卸载后按本机架构重装: npm uninstall -g @anthropic-ai/claude-code 再 \
                 npm i -g @anthropic-ai/claude-code --registry=https://registry.npmmirror.com"
            );
        }
        format!("调起宿主机 claude CLI 失败: {}", e)
    })
}

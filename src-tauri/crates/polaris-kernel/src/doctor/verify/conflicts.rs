use super::*;

// ───────────────────────── 安装冲突扫描 ─────────────────────────

pub(super) fn conflict(
    key: &str,
    severity: &str,
    title: &str,
    detail: String,
    paths: Vec<String>,
    fixable: bool,
) -> EnvConflict {
    EnvConflict {
        key: key.into(),
        severity: severity.into(),
        title: title.into(),
        detail,
        paths,
        fixable,
    }
}

/// 用户**自己**设过的 ANTHROPIC_*/CLAUDE_* 环境变量 (持久化那一层, 不是本进程被应用改过的那份)。
/// Windows 读注册表用户级; 类 Unix 扫 shell 配置。返回 (变量名, 掩码后的值)。
pub(super) fn external_managed_vars() -> Vec<(String, String)> {
    const KEYS: [&str; 5] = [
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_AUTH_TOKEN",
        "ANTHROPIC_BASE_URL",
        "ANTHROPIC_MODEL",
        "CLAUDE_CONFIG_DIR",
    ];
    #[cfg(windows)]
    {
        // 一次 PowerShell 调用把 5 个用户级变量全取回来 (逐个取要开 5 个进程, 太慢)
        let names = KEYS
            .iter()
            .map(|k| format!("'{k}'"))
            .collect::<Vec<_>>()
            .join(",");
        let script = format!(
            "$o=@{{}}; foreach($n in {names}){{ $v=[Environment]::GetEnvironmentVariable($n,'User'); \
if($v){{ $o[$n]=$v }} }}; $o | ConvertTo-Json -Compress"
        );
        let mut cmd = Command::new("powershell");
        cmd.args(["-NoProfile", "-NonInteractive", "-Command", &script]);
        cmd.stdin(Stdio::null());
        no_window(&mut cmd);
        let Some(out) = output_with_timeout(cmd, PROBE_TIMEOUT) else {
            return Vec::new();
        };
        let body = String::from_utf8_lossy(&out.stdout);
        let Ok(v) = serde_json::from_str::<serde_json::Value>(body.trim()) else {
            return Vec::new();
        };
        let Some(map) = v.as_object() else {
            return Vec::new();
        };
        return map
            .iter()
            .filter_map(|(k, val)| {
                let s = val.as_str()?.trim();
                (!s.is_empty()).then(|| {
                    // base_url / model 不是秘密, 明文展示更有用; key/token 一律掩码
                    let shown = if k.ends_with("_KEY") || k.ends_with("_TOKEN") {
                        mask(s)
                    } else {
                        s.to_string()
                    };
                    (k.clone(), shown)
                })
            })
            .collect();
    }
    #[cfg(not(windows))]
    {
        let Some(home) = home_dir() else {
            return Vec::new();
        };
        let mut found: BTreeMap<String, String> = BTreeMap::new();
        for rc in [".zshrc", ".zprofile", ".bash_profile", ".bashrc", ".profile"] {
            let Ok(txt) = std::fs::read_to_string(home.join(rc)) else {
                continue;
            };
            for line in txt.lines() {
                let l = line.trim();
                if l.starts_with('#') {
                    continue;
                }
                let l = l.strip_prefix("export ").unwrap_or(l);
                for k in KEYS {
                    let Some(rest) = l.strip_prefix(k).and_then(|r| r.strip_prefix('=')) else {
                        continue;
                    };
                    let val = rest.trim().trim_matches(['"', '\'']).trim();
                    if val.is_empty() {
                        continue;
                    }
                    let shown = if k.ends_with("_KEY") || k.ends_with("_TOKEN") {
                        mask(val)
                    } else {
                        val.to_string()
                    };
                    found.insert(format!("{k}  ({rc})"), shown);
                }
            }
        }
        return found.into_iter().collect();
    }
}

/// 「装重了」这一族冲突的**纯逻辑**部分: Store 别名遮挡 / 多份并存 / 版本不一致 /
/// 应用与终端跑的不是同一份。抽成不碰全局态的函数 —— 探测(`which`/`--version`)在外面做完喂进来,
/// 这样单测能用真实文件摆出各种冲突局面来验, 而不是靠「本机恰好装重了」才测得到。
///
/// `version_of` 由调用方注入 (真实实现是跑 `--version`; 测试里给个查表函数)。
pub(super) fn install_conflicts(
    all: &[PathBuf],
    app_exe: Option<&Path>,
    term_first: Option<&Path>,
    version_of: &dyn Fn(&Path) -> Option<String>,
) -> Vec<EnvConflict> {
    let mut out: Vec<EnvConflict> = Vec::new();

    // Store 执行别名: 排在 PATH 前面就会把终端里的 `claude` 劫持到应用商店
    let aliases: Vec<&PathBuf> = all.iter().filter(|p| is_app_exec_alias(p)).collect();
    if !aliases.is_empty() {
        out.push(conflict(
            "claude-store-alias",
            "medium",
            "存在 Microsoft Store 执行别名占位",
            "`WindowsApps` 下的 0 字节占位符会抢在真身之前被 PATH 命中 —— 终端里敲 `claude` \
             可能直接跳到应用商店。可在「设置 → 应用 → 应用执行别名」里关掉它。"
                .into(),
            aliases.iter().map(|p| to_fwd(p)).collect(),
            false,
        ));
    }

    // ── ② 按安装根分组: 一份安装的多个入口归一类, 真正的多份并存才算冲突 ──
    let real: Vec<PathBuf> = all
        .iter()
        .filter(|p| !is_app_exec_alias(p))
        .cloned()
        .collect();
    let mut groups: BTreeMap<String, (PathBuf, Vec<PathBuf>)> = BTreeMap::new();
    for p in &real {
        let root = install_root(p);
        groups
            .entry(norm(&root))
            .or_insert_with(|| (root, Vec::new()))
            .1
            .push(p.clone());
    }

    // 每份安装探一次版本 (用它自己那个能直接跑的入口)
    let mut versions: BTreeMap<String, Option<String>> = BTreeMap::new();
    for (k, (_, entries)) in &groups {
        let v = best_entry(entries).and_then(|e| version_of(&e));
        versions.insert(k.clone(), v);
    }

    if groups.len() > 1 {
        let lines: Vec<String> = groups
            .iter()
            .map(|(k, (root, _))| {
                let v = versions
                    .get(k)
                    .cloned()
                    .flatten()
                    .unwrap_or_else(|| "版本未知".into());
                format!("{} — {v}  [{}]", to_fwd(root), root_label(root))
            })
            .collect();
        out.push(conflict(
            "claude-multi",
            "high",
            &format!("检测到 {} 份 Claude Code 并存", groups.len()),
            "多份并存时, 到底跑哪份完全取决于 PATH 顺序 —— 常见后果是「更新完版本号纹丝不动」\
             (更新的是 A, 跑的是 B)。建议只保留一份: 若用应用的一键安装, 保留 `~/.local/bin` 那份, \
             用 `npm uninstall -g @anthropic-ai/claude-code` 删掉 npm 那份。"
                .into(),
            lines,
            false,
        ));

        // 版本还不一致 → 更凶: 行为在两个终端里都不一样
        let distinct: std::collections::BTreeSet<String> =
            versions.values().flatten().cloned().collect();
        if distinct.len() > 1 {
            out.push(conflict(
                "claude-version-mismatch",
                "high",
                "并存的 Claude Code 版本不一致",
                "同一台机器上跑出两个不同版本 —— 应用里和终端里的行为、可用参数都会不同。\
                 删到只剩一份即可。"
                    .into(),
                distinct.into_iter().collect(),
                false,
            ));
        }
    }

    // ── ③ 应用解析到的那份 vs 终端会跑的那份 ──
    if let (Some(app), Some(t0)) = (app_exe, term_first) {
        let a = norm(&install_root(app));
        let b = norm(&install_root(t0));
        if a != b {
            out.push(conflict(
                "claude-app-vs-terminal",
                "high",
                "应用与终端跑的不是同一份 Claude Code",
                "应用里发消息用的是前者, 你在终端里敲 `claude` 用的是后者 —— 版本、登录状态、\
                 MCP 配置都可能对不上。删掉多余的那份, 或调整 PATH 顺序让两边一致。"
                    .into(),
                vec![
                    format!("应用: {}", to_fwd(app)),
                    format!("终端: {}", to_fwd(t0)),
                ],
                false,
            ));
        }
    }

    out
}

/// 扫描全部安装冲突。`term` 是「新开终端」那次探测的结果 —— 判「终端与应用跑的不是同一份」全靠它。
pub(super) fn scan_conflicts(
    app_exe: Option<&Path>,
    term: &TerminalProbe,
    broken_cfg: &[(String, String)],
) -> Vec<EnvConflict> {
    // ── ①②③ 收集本机所有 claude 入口 (PATH 命中 + 终端命中 + 已知候选), 去重后交给纯逻辑 ──
    let mut all: Vec<PathBuf> = which_all("claude");
    all.extend(term.hits.iter().cloned().filter(|p| p.exists()));
    all.extend(claude_candidates().into_iter().filter(|p| p.exists()));
    let mut seen = std::collections::HashSet::new();
    all.retain(|p| seen.insert(norm(p)));

    let mut out = install_conflicts(
        &all,
        app_exe,
        term.hits.first().map(|p| p.as_path()),
        &|p| probe_version_at(p, &["--version"]),
    );

    // ── ④ Node / npm 侧的冲突 ──
    let node_dirs: std::collections::BTreeSet<String> = which_all("node")
        .iter()
        .filter(|p| !is_app_exec_alias(p))
        .filter_map(|p| p.parent().map(norm))
        .collect();
    if node_dirs.len() > 1 {
        out.push(conflict(
            "node-multi",
            "low",
            &format!("PATH 上有 {} 份 Node.js", node_dirs.len()),
            "多版本共存 (nvm / fnm / 系统装 混用) 本身没问题, 但 `npm i -g` 装的 claude 只对\
             当前那一份可见 —— 切换 Node 版本后可能突然「找不到 claude」。"
                .into(),
            node_dirs.iter().map(|s| s.replace('\\', "/")).collect(),
            false,
        ));
    }
    // npm 装的 claude 需要 Node 18+
    let npm_installed = app_exe.is_some_and(|p| {
        p.components().any(|c| c.as_os_str() == "node_modules")
            || npm_global_prefix().is_some_and(|pre| p.parent() == Some(pre.as_path()))
    });
    if npm_installed {
        if let Some(major) = probe_version("node", &["--version"])
            .and_then(|v| {
                v.trim()
                    .trim_start_matches('v')
                    .split('.')
                    .next()
                    .map(|s| s.to_string())
            })
            .and_then(|s| s.parse::<u32>().ok())
        {
            if major < 18 {
                out.push(conflict(
                    "node-too-old",
                    "medium",
                    &format!("Node.js 版本过低 (v{major})"),
                    "npm 方式安装的 Claude Code 需要 Node 18 以上, 更低版本会在启动时报语法错误。\
                     升级 Node, 或改用应用的一键安装 (直抓平台包, 完全不依赖 Node)。"
                        .into(),
                    vec![format!("node v{major}")],
                    false,
                ));
            }
        }
    }
    // npm 全局前缀不在用户 PATH → npm 装的任何全局命令在新终端里都找不到
    #[cfg(windows)]
    if let (Some(prefix), Some(user_path)) = (npm_global_prefix(), read_user_path()) {
        if !path_contains_dir(&user_path, &prefix.to_string_lossy()) {
            out.push(conflict(
                "npm-prefix-off-path",
                "medium",
                "npm 全局目录不在用户 PATH 里",
                "`npm i -g` 装的命令 (含 claude) 在新开的终端里都会「找不到」。点「修复 PATH」\
                 或手动把该目录加进用户环境变量。"
                    .into(),
                vec![to_fwd(&prefix)],
                true,
            ));
        }
    }

    // ── ⑤ 外部设置的 ANTHROPIC_* / CLAUDE_CONFIG_DIR ──
    let ext = external_managed_vars();
    if !ext.is_empty() {
        out.push(conflict(
            "external-anthropic-env",
            "medium",
            "系统里有你自己设的 Claude 相关环境变量",
            "这些变量会被 claude 直接读取, 可能盖掉应用内「供应商」页选的那家 —— 表现为\
             「面板选了 A, 实际走了 B」或反复报鉴权失败。若不是刻意为之, 建议删掉它们, \
             把供应商配置交给应用统一管。"
                .into(),
            ext.into_iter().map(|(k, v)| format!("{k} = {v}")).collect(),
            false,
        ));
    }

    // ── ⑥ 配置文件损坏 (与 step_config 同源, 这里再登记一条冲突以便统一展示严重度) ──
    if !broken_cfg.is_empty() {
        out.push(conflict(
            "claude-config-broken",
            "high",
            "Claude 配置文件损坏",
            "不是合法 JSON 的配置会让 claude 启动即崩。备份后删掉损坏的那个文件, claude 会自动重建。"
                .into(),
            broken_cfg.iter().map(|(p, e)| format!("{p} — {e}")).collect(),
            false,
        ));
    }

    // 严重度排序: high → medium → low
    let rank = |s: &str| match s {
        "high" => 0,
        "medium" => 1,
        _ => 2,
    };
    out.sort_by_key(|c| rank(&c.severity));
    out
}


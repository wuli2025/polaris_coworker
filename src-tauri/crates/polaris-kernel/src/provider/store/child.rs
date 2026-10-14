use super::*;
/// 同步当前进程 env: spawn 出去的 claude 子进程会继承父进程 env, 而进程 env 通常**优先于**
/// settings.json 的 env(实测), 不先清后置就会出现:
///   ① 切到 M3 → 进程被 set 了 ANTHROPIC_BASE_URL=minimaxi
///   ② 切回官方 → 只清了 settings, 父进程残留仍把 claude 拖到 minimaxi → 一直只能用 M3
/// 先按 MANAGED_ENV_KEYS 全清, 再把新 cfg.env 写进当前进程 —— 切换结果确定。
/// 隔离模式下这就是切换的**唯一**生效通道。
pub(super) fn apply_process_env(cfg: &Value) {
    for k in MANAGED_ENV_KEYS {
        std::env::remove_var(k);
    }
    if let Some(src_env) = cfg.get("env").and_then(|e| e.as_object()) {
        for (k, val) in src_env {
            if let Some(s) = val.as_str() {
                std::env::set_var(k, s);
            }
        }
    }
}

// ───────────────── 隔离模式·私有 claude 配置目录(会话账本隔离) ─────────────────
//
// 配置隔离(进程 env)只解决「外部 CLI 用哪家」; 但 claude 的会话 jsonl 仍写进共享
// `~/.claude/projects/`, cc-switch 这类监控按分钟扫那里记账 → 监控里永远有
// Polaris 自动任务的 MiniMax 行。深隔离: 隔离模式下跑**非官方**供应商时, 给子进程
// CLAUDE_CONFIG_DIR=~/Polaris/claude-home, 会话记录/customApiKeyResponses 全落私有
// 目录, 共享账本只剩用户本人的会话。官方档仍用共享 ~/.claude —— OAuth 凭证在那,
// 且官方会话本来就该记在用户自己的账上。

/// 隔离模式下第三方/Codex 任务的私有 claude 配置目录。
pub fn private_claude_home() -> Option<PathBuf> {
    UserDirs::new().map(|u| u.home_dir().join("Polaris").join("claude-home"))
}

/// 所有已存供应商 key 的「尾 20 字符」—— claude 在 .claude.json 的
/// customApiKeyResponses.approved 里就是按这个尾巴记录「该 key 已被用户批准」。
/// 预先播种进私有目录, headless 首启不会因 key 审批交互被卡死。
pub(super) fn provider_key_tails(store: &Store) -> Vec<String> {
    let mut tails: Vec<String> = Vec::new();
    for it in build_views(store) {
        for field in [DEFAULT_TOKEN_FIELD, API_KEY_FIELD] {
            let tok = cfg_env_str(&it.settings_config, field);
            let tok = tok.trim();
            if tok.is_empty() {
                continue;
            }
            let chars: Vec<char> = tok.chars().collect();
            let tail: String = chars[chars.len().saturating_sub(20)..].iter().collect();
            if !tails.contains(&tail) {
                tails.push(tail);
            }
        }
    }
    tails
}

/// 创建并播种私有配置目录(幂等, 内容没变就不落盘):
/// .claude.json 里 hasCompletedOnboarding=true + 全部供应商 key 尾巴进 approved。
pub(super) fn ensure_private_home(home: &Path, store: &Store) -> Result<(), String> {
    fs::create_dir_all(home).map_err(|e| format!("创建私有 claude 目录失败: {e}"))?;
    let path = home.join(".claude.json");
    let mut root: Value = if path.exists() {
        let txt = fs::read_to_string(&path).unwrap_or_default();
        serde_json::from_str(&txt).unwrap_or_else(|_| json!({}))
    } else {
        json!({})
    };
    if !root.is_object() {
        root = json!({});
    }
    let obj = root.as_object_mut().unwrap();
    let mut changed = false;

    if obj.get("hasCompletedOnboarding").and_then(|v| v.as_bool()) != Some(true) {
        obj.insert("hasCompletedOnboarding".into(), Value::Bool(true));
        changed = true;
    }

    let resp = obj
        .entry("customApiKeyResponses".to_string())
        .or_insert_with(|| json!({ "approved": [], "rejected": [] }));
    if !resp.is_object() {
        *resp = json!({ "approved": [], "rejected": [] });
        changed = true;
    }
    let resp = resp.as_object_mut().unwrap();
    let approved = resp
        .entry("approved".to_string())
        .or_insert_with(|| json!([]));
    if !approved.is_array() {
        *approved = json!([]);
        changed = true;
    }
    let arr = approved.as_array_mut().unwrap();
    for tail in provider_key_tails(store) {
        if !arr.iter().any(|v| v.as_str() == Some(tail.as_str())) {
            arr.push(Value::String(tail));
            changed = true;
        }
    }

    if changed {
        let txt = serde_json::to_string_pretty(&root)
            .map_err(|e| format!("序列化私有 .claude.json 失败: {e}"))?;
        atomic_write(&path, &txt).map_err(|e| format!("写私有 .claude.json 失败: {e}"))?;
    }
    Ok(())
}

/// 给宿主机 spawn 的 claude 子进程套供应商作用域。chat.rs / kb.rs 所有宿主 spawn
/// 点统一调这一个入口; 不满足深隔离条件(联动 / 官方档 / env 没有 base_url)时什么都不做,
/// 子进程照旧用共享 ~/.claude。
pub fn scope_child_claude(cmd: &mut Command) {
    let store = STORE.read().clone();
    if store.link_global {
        return;
    }
    if store.current_id.is_empty() || store.current_id == "claude-official" {
        return;
    }
    // 进程 env 没有 base_url = 实际跑在官方档(如配置不全回落), 不隔离。
    if std::env::var("ANTHROPIC_BASE_URL")
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        return;
    }
    let Some(home) = private_claude_home() else {
        return;
    };
    // 播种失败宁可回落共享目录, 也不让任务带着半成品配置首启卡死。
    if ensure_private_home(&home, &store).is_err() {
        return;
    }
    cmd.env("CLAUDE_CONFIG_DIR", &home);
}

/// 按对话选定的供应商 id, 把该供应商配置**逐命令**注入到这条 claude 子进程 —— 实现
/// 「每个对话各用各的 API、互不串台」的真隔离。与 `scope_child_claude`(吃全局当前)不同:
/// 这里不依赖、也不改全局进程 env, 而是直接在 `Command` 上先清掉继承来的受管键、再套上
/// **本对话这家**的 env, 因此并发的多条对话(各绑不同供应商)同时在跑也不会互相覆盖。
///
/// 语义:
/// - `None` / `""` / `"auto"` —— 「Auto」档: 不做逐命令注入, 回落到 `scope_child_claude`
///   (沿用应用全局当前供应商, 继承进程 env)。新对话默认即此, 行为与旧版完全一致。
/// - 具体 id —— 解析该供应商(codex 会顺带确保本地翻译代理在跑), 清受管键后注入它的 env;
///   非官方第三方再套私有 `CLAUDE_CONFIG_DIR`(会话账本不污染共享 ~/.claude)。
/// - 找不到该 id / 未配 key / 未授权 —— 安全回落到全局当前, 绝不让对话因配置缺失发不出去。
pub fn scope_child_claude_by_id(cmd: &mut Command, provider_id: Option<&str>) {
    let id = provider_id.map(|s| s.trim()).unwrap_or("");
    if id.is_empty() || id == "auto" {
        scope_child_claude(cmd);
        return;
    }
    let store = STORE.read().clone();
    let views = build_views(&store);
    let Some(v) = views.iter().find(|v| v.id == id) else {
        scope_child_claude(cmd);
        return;
    };
    // 解析待生效 env(codex / OpenAI 协议 / 路由总开关下会确保本地路由在跑)。
    // 未配 key / 未授权 → 回落全局当前, 不阻断对话。
    let cfg = match cfg_for_view(v, store.route_local) {
        Ok(c) => c,
        Err(_) => {
            scope_child_claude(cmd);
            return;
        }
    };
    // 逐命令注入: 先把继承自 Polaris 进程的受管键全清(否则全局那家的 base_url/token 会漏进来),
    // 再套本对话这家 —— 这条 claude 从此自带完整配置, 与全局开关、与其它并发对话彻底解耦。
    for k in MANAGED_ENV_KEYS {
        cmd.env_remove(k);
    }
    if let Some(env) = cfg.get("env").and_then(|e| e.as_object()) {
        for (k, val) in env {
            if let Some(s) = val.as_str() {
                cmd.env(k, s);
            }
        }
    }
    // 会话账本隔离: 非官方第三方 → 私有 CLAUDE_CONFIG_DIR; 官方仍用共享 ~/.claude(OAuth 凭据在那)。
    let base = cfg
        .get("env")
        .and_then(|e| e.get("ANTHROPIC_BASE_URL"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if v.kind != "official" && !base.is_empty() {
        if let Some(home) = private_claude_home() {
            if ensure_private_home(&home, &store).is_ok() {
                cmd.env("CLAUDE_CONFIG_DIR", &home);
            }
        }
    }
}


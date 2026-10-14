use super::*;
pub(super) fn claude_settings_path() -> Option<PathBuf> {
    claude_dir().map(|d| d.join("settings.json"))
}

pub(super) fn read_live_env() -> Map<String, Value> {
    let Some(path) = claude_settings_path() else {
        return Map::new();
    };
    let Ok(txt) = fs::read_to_string(&path) else {
        return Map::new();
    };
    let Ok(v) = serde_json::from_str::<Value>(&txt) else {
        return Map::new();
    };
    v.get("env")
        .and_then(|e| e.as_object())
        .cloned()
        .unwrap_or_default()
}

/// 隔离模式下判定「全局 settings.json 的受管 env 是不是我们(联动时代/旧版本)写的」。
/// 证据从强到弱:
/// ① POLARIS_LINKED 戳(新版联动写全局时盖的);
/// ② base_url 命中当前供应商(旧规则, 保留兼容);
/// ③ base_url 命中任一已知供应商 **且** token 与我们存的该家 key 一致 —— 用户已切回
///    官方、残留还指着上一家时, ①②全失效, 残留永远清不掉(实测踩坑「切一次就切不
///    回来」), 全靠这条兜底; key 不同则视为用户自己在终端配的, 不动。
/// ④ base 已清但模型钉选残留(等于我们某家钉的模型) —— 官方端点带着 MiniMax-M3
///    这种钉选必然 4xx, 属我们的残留, 清。
pub(super) fn global_env_is_ours(live: &Map<String, Value>, views: &[ProviderView], store: &Store) -> bool {
    if live.contains_key("POLARIS_LINKED") {
        return true;
    }
    let live_str = |k: &str| {
        live.get(k)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string()
    };
    let live_base = normalize_url(&live_str("ANTHROPIC_BASE_URL"));
    if !live_base.is_empty() {
        if let Some(cur) = views.iter().find(|v| v.id == store.current_id) {
            if normalize_url(&cur.base_url) == live_base {
                return true;
            }
        }
        return views.iter().any(|v| {
            !v.auth_token.trim().is_empty()
                && !v.base_url.is_empty()
                && normalize_url(&v.base_url) == live_base
                && [DEFAULT_TOKEN_FIELD, API_KEY_FIELD]
                    .iter()
                    .any(|f| live_str(f) == v.auth_token.trim())
        });
    }
    MODEL_ENV_KEYS.iter().any(|k| {
        let m = live_str(k);
        !m.is_empty()
            && views
                .iter()
                .any(|v| cfg_env_str(&v.settings_config, k).trim() == m)
    })
}

/// 隔离模式的残留体检: 全局 settings.json 里若还躺着我们写的受管键, 清回官方。
/// init 启动时和每次切换都跑一遍 —— 无证据时只读不写, 幂等且零成本。
pub(super) fn purge_global_residue(views: &[ProviderView], store: &Store) {
    let live = read_live_env();
    if !live.is_empty() && global_env_is_ours(&live, views, store) {
        let _ = apply_settings_config(&json!({ "env": {} }));
    }
}

/// 把供应商 settings_config 合并写进 live settings.json：
/// 先从 live 清掉受管 env/top 键，再套用 cfg 的 env 与顶层键，其余 live 键原样保留。
pub(super) fn apply_settings_config(cfg: &Value) -> Result<(), String> {
    // 整条「读 settings.json → 合并 → 写回」串行化, 防并发切换交错撕裂用户实时配置。
    let _io = IO_LOCK.lock();
    let path = claude_settings_path().ok_or_else(|| "无法定位用户主目录".to_string())?;
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let mut root: Value = if path.exists() {
        let txt = fs::read_to_string(&path).map_err(|e| format!("读 settings.json 失败: {e}"))?;
        if txt.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str(&txt)
                .map_err(|e| format!("settings.json 不是合法 JSON, 已中止以免破坏: {e}"))?
        }
    } else {
        json!({})
    };

    if path.exists() {
        let bak = path.with_extension("json.polaris.bak");
        if !bak.exists() {
            let _ = fs::copy(&path, &bak);
        }
    }

    if !root.is_object() {
        root = json!({});
    }
    let obj = root.as_object_mut().unwrap();

    // 清受管顶层键
    for k in MANAGED_TOP_KEYS {
        obj.remove(*k);
    }
    // env: 清受管键后套用 cfg.env
    let env = obj.entry("env".to_string()).or_insert_with(|| json!({}));
    if !env.is_object() {
        *env = json!({});
    }
    let env = env.as_object_mut().unwrap();
    for k in MANAGED_ENV_KEYS {
        env.remove(*k);
    }
    if let Some(src_env) = cfg.get("env").and_then(|e| e.as_object()) {
        for (k, v) in src_env {
            env.insert(k.clone(), v.clone());
        }
    }
    // 真正路由了全局(env 带 base_url)就盖 POLARIS_LINKED 戳: 日后隔离模式的净化
    // 凭这个戳就能确认残留是我们写的, 不再依赖「当前供应商恰好没换」这种弱证据。
    let routed = env
        .get("ANTHROPIC_BASE_URL")
        .and_then(|v| v.as_str())
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);
    if routed {
        env.insert("POLARIS_LINKED".into(), Value::String("1".into()));
    }
    // 顶层键 (除 env) 套用
    if let Some(src) = cfg.as_object() {
        for (k, v) in src {
            if k == "env" {
                continue;
            }
            obj.insert(k.clone(), v.clone());
        }
    }

    let txt = serde_json::to_string_pretty(&root)
        .map_err(|e| format!("序列化 settings.json 失败: {e}"))?;
    atomic_write(&path, &txt).map_err(|e| format!("写 settings.json 失败: {e}"))?;
    Ok(())
}


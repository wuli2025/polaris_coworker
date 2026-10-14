use super::*;
/// 给「生图」用的画像：返回 (可用的生图家展示名, 是否真能生图)。
///
/// 本表(聊天供应商坞)里 55 家全是文本/代码大模型,**没有一个能生图** —— 生图配置住在
/// 独立的生图坞(`image_store.rs`,理由见其文件头)。这里只是把它转出来。
///
/// 旧实现是「settings.json 或进程 env 里有非空 `OPENAI_API_KEY` 就算支持」—— 那是张**空头
/// 支票**:环境变量里有个 key 不代表我们真会去调它(当时压根没有生图调用路径),而
/// `prompt.rs` 却照着它跟用户承诺「可以在 API 供应商里配置图像 API」。现在按**真配了生图家
/// 且填了 Key** 判定,承诺才对得上实现。
pub fn image_gen_capability() -> (String, bool) {
    match image_store::current_image_config() {
        Some(c) => (format!("{}({})", c.name, c.model), true),
        None => (String::new(), false),
    }
}

// ───────────────────────── Commands: 供应商 ─────────────────────────

/// 供应商列表。**出口一律打码**(见 `mask_secret` 上方的大注释):这条命令在 server 壳里
/// 是全量命令面的一员, 免口令的 NAS 版一问就能把全部上游 key 明文取走。
///
/// 内部要用真 key 的路径(cfg_for_view / route_target / 各条借 key 链)走的都是
/// `build_views` 或直接读 STORE, 不经过这里 —— 打码不影响它们。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn provider_list() -> Result<ProviderListResult, String> {
    let store = STORE.read().clone();
    let mut providers = build_views(&store);
    let current_id = detect_current(&providers, &store);
    for p in providers.iter_mut() {
        mask_view(p);
    }
    Ok(ProviderListResult {
        providers,
        current_id,
        link_global: store.link_global && !pi_translate::is_pi(),
        route_local: store.route_local && !pi_translate::is_pi(),
    })
}

/// 把供应商视图换算成待生效的 settings_config(codex / OpenAI 协议会顺带拉起本地路由)。
/// `route_all` = 本地路由总开关(store.route_local): 开着时 Anthropic 兼容供应商
/// 也统一改走 127.0.0.1 本地路由透传(cc-switch 式代理模式)。
pub(super) fn cfg_for_view(v: &ProviderView, route_all: bool) -> Result<Value, String> {
    if v.kind == "copilot" {
        return Err("GitHub Copilot 需 GitHub OAuth + token 交换, 本地路由暂未覆盖".to_string());
    }
    if v.kind == "codex" {
        // Codex(ChatGPT) → 路由到本地翻译代理: 先确认已授权, 再拉起路由并把
        // ANTHROPIC_BASE_URL 指到 127.0.0.1:port/p/codex, claude 即透明用上 ChatGPT 订阅。
        let authed = codex_auth_path()
            .map(|p| codex_auth_has_tokens(&p))
            .unwrap_or(false);
        if !authed {
            return Err("请先授权 ChatGPT (Codex), 再切换到它".to_string());
        }
        let port = crate::integrations::codex_proxy::ensure_running()?;
        return Ok(local_route_config(port, "codex", "gpt5.6-sol"));
    }
    if v.protocol == "openai" {
        // OpenAI 协议(GPT API / 各家 OpenAI 兼容网关)→ 同样走本地路由转发:
        // claude 说 Anthropic 协议发到 127.0.0.1:port/p/{id}, 路由实时查 store 里
        // 这家的 base_url + key, 翻译成 chat/completions 转发 —— cc-switch 式本地转发。
        if v.auth_token.trim().is_empty() {
            return Err("该供应商尚未配置 API Key, 请先在弹窗中填写".to_string());
        }
        if normalize_url(&v.base_url).is_empty() {
            return Err("该供应商尚未配置请求地址, 请先在弹窗中填写".to_string());
        }
        let port = crate::integrations::codex_proxy::ensure_running()?;
        let model = pinned_model_or_default(&v.settings_config);
        return Ok(local_route_config(port, &v.id, &model));
    }
    if v.kind == "official" {
        return Ok(json!({ "env": {} }));
    }
    if v.auth_token.trim().is_empty() {
        return Err("该供应商尚未配置 API Key, 请先在弹窗中填写".to_string());
    }
    if route_all && !normalize_url(&v.base_url).is_empty() {
        // 本地路由总开关: Anthropic 兼容供应商也统一走 /p/{id} 透传。base 指到本地路由,
        // key 从 env 里拿掉换占位(真 key 由路由每请求实时注入 —— 改 key 即刻生效,
        // 且不再进子进程 env); 模型钉选等其余 env 原样保留。
        let port = crate::integrations::codex_proxy::ensure_running()?;
        let mut cfg = v.settings_config.clone();
        if !cfg.is_object() {
            cfg = json!({});
        }
        let obj = cfg.as_object_mut().unwrap();
        let env = obj.entry("env".to_string()).or_insert_with(|| json!({}));
        if !env.is_object() {
            *env = json!({});
        }
        let env = env.as_object_mut().unwrap();
        env.insert(
            "ANTHROPIC_BASE_URL".into(),
            Value::String(format!("http://127.0.0.1:{port}/p/{}", v.id)),
        );
        env.remove("ANTHROPIC_AUTH_TOKEN");
        env.remove("ANTHROPIC_API_KEY");
        let field = if v.token_field.is_empty() {
            DEFAULT_TOKEN_FIELD
        } else {
            v.token_field.as_str()
        };
        env.insert(field.into(), Value::String(LOCAL_ROUTE_TOKEN.into()));
        return Ok(cfg);
    }
    Ok(v.settings_config.clone())
}


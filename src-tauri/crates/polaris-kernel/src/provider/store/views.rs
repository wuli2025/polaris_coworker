use super::*;
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderView {
    pub id: String,
    pub name: String,
    pub note: String,
    pub base_url: String,
    pub token_field: String,
    pub category: String,
    pub website_url: String,
    pub color: String,
    pub kind: String,
    /// 上游协议: "" = Anthropic 兼容(claude 直连) | "openai" = OpenAI chat/completions
    /// (经本地路由翻译) | "chatgpt" = ChatGPT 订阅(codex, 经本地路由)。
    pub protocol: String,
    pub is_preset: bool,
    pub has_key: bool,
    pub auth_token: String,
    pub settings_config: Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderListResult {
    pub providers: Vec<ProviderView>,
    pub current_id: String,
    /// true = 联动(写全局 settings.json), false = 隔离(仅 Polaris 内生效)
    pub link_global: bool,
    /// 本地路由总开关: true = 所有供应商统一经 127.0.0.1 本地路由转发(热切换)
    pub route_local: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInput {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub website_url: String,
    #[serde(default)]
    pub token_field: Option<String>,
    /// ""/None = Anthropic 兼容(直连); "openai" = OpenAI 协议(经本地路由转发)
    #[serde(default)]
    pub protocol: Option<String>,
    #[serde(default)]
    pub settings_config: Value,
}

pub(super) fn normalize_url(u: &str) -> String {
    u.trim().trim_end_matches('/').to_string()
}

pub(super) fn make_view(
    id: &str,
    name: &str,
    note: &str,
    token_field: &str,
    category: &str,
    kind: &str,
    protocol: &str,
    is_preset: bool,
    preset_base: &str,
    website: &str,
    cfg: Value,
) -> ProviderView {
    let mut cfg = cfg;
    if let Ok(Some(c)) = credential_file::read(pi_translate::credential_id(id)) {
        if let Some(key) = c.get("key").and_then(Value::as_str) {
            if !cfg.is_object() { cfg = json!({}); }
            if !cfg["env"].is_object() { cfg["env"] = json!({}); }
            cfg["env"][token_field] = json!(key);
        }
    }
    let env_base = cfg_env_str(&cfg, "ANTHROPIC_BASE_URL");
    let base_url = if env_base.is_empty() {
        preset_base.to_string()
    } else {
        env_base
    };
    let token = cfg_env_str(&cfg, token_field);
    let mut has_key = match kind {
        "official" => true,
        "codex" | "copilot" => false,
        _ => !token.is_empty(),
    };
    if pi_translate::is_pi() && matches!(kind, "official" | "codex") {
        has_key = credential_file::read(pi_translate::credential_id(id)).ok().flatten().is_some();
    }
    // 预设 kind 即协议来源; 自定义供应商由存储的 protocol 决定。
    let protocol = match kind {
        "codex" => "chatgpt",
        "openai" => "openai",
        _ => protocol,
    };
    let website = if website.is_empty() {
        website_from_base(&base_url)
    } else {
        website.to_string()
    };
    ProviderView {
        id: id.to_string(),
        name: name.to_string(),
        note: note.to_string(),
        base_url,
        token_field: token_field.to_string(),
        category: category.to_string(),
        website_url: website,
        color: color_for(category).to_string(),
        kind: kind.to_string(),
        protocol: protocol.to_string(),
        is_preset,
        has_key,
        auth_token: token,
        settings_config: cfg,
    }
}

pub(crate) fn build_views(store: &Store) -> Vec<ProviderView> {
    let mut out: Vec<ProviderView> = Vec::with_capacity(PRESETS.len() + store.items.len());

    for p in PRESETS {
        let stored = store.items.iter().find(|i| i.id == p.id);
        let token_field = stored
            .map(|s| s.token_field.clone())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| p.token_field.to_string());
        let cfg = stored
            .map(|s| s.settings_config.clone())
            .unwrap_or_else(|| default_config(p.base_url, &token_field, ""));
        let note = stored.map(|s| s.note.as_str()).unwrap_or("");
        out.push(make_view(
            p.id,
            p.name,
            note,
            &token_field,
            p.category,
            p.kind,
            stored.map(|s| s.protocol.as_str()).unwrap_or(""),
            true,
            p.base_url,
            "",
            cfg,
        ));
    }

    for it in &store.items {
        if preset_by_id(&it.id).is_some() {
            continue; // 预设覆盖已在上方合并
        }
        let token_field = if it.token_field.is_empty() {
            DEFAULT_TOKEN_FIELD.to_string()
        } else {
            it.token_field.clone()
        };
        out.push(make_view(
            &it.id,
            &it.name,
            &it.note,
            &token_field,
            "custom",
            "custom",
            &it.protocol,
            false,
            "",
            &it.website_url,
            it.settings_config.clone(),
        ));
    }

    out
}

pub(super) fn detect_current(views: &[ProviderView], store: &Store) -> String {
    if pi_translate::is_pi() { return store.current_id.clone(); }
    // 联动: 真相在全局 settings.json(外部 cc-switch 等改动也能被察觉);
    // 隔离: 全局与我们无关, 真相在本进程 env(apply_process_env 设的)。
    let live_base = if store.link_global {
        read_live_env()
            .get("ANTHROPIC_BASE_URL")
            .and_then(|v| v.as_str())
            .map(normalize_url)
            .unwrap_or_default()
    } else {
        std::env::var("ANTHROPIC_BASE_URL")
            .map(|s| normalize_url(&s))
            .unwrap_or_default()
    };

    if live_base.is_empty() {
        if store.current_id == "claude-official" || store.current_id.is_empty() {
            return "claude-official".to_string();
        }
        if let Some(v) = views.iter().find(|v| v.id == store.current_id) {
            if normalize_url(&v.base_url).is_empty() {
                return v.id.clone();
            }
        }
        return "claude-official".to_string();
    }

    if let Some(v) = views
        .iter()
        .find(|v| !v.base_url.is_empty() && normalize_url(&v.base_url) == live_base)
    {
        return v.id.clone();
    }
    if !store.current_id.is_empty() && views.iter().any(|v| v.id == store.current_id) {
        return store.current_id.clone();
    }
    String::new()
}

// ───────────────────────── ~/.claude/settings.json 读写 ─────────────────────────

pub(crate) fn claude_dir() -> Option<PathBuf> {
    UserDirs::new().map(|u| u.home_dir().join(".claude"))
}

pub fn pi_views() -> Vec<ProviderView> { build_views(&STORE.read()) }

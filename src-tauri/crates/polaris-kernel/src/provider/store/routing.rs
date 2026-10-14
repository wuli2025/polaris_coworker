use super::*;
/// 往 settings_config 的 env 里钉模型:把 MODEL_ENV_KEYS 四档全设成同一个 model id。
/// model 为空则原样返回(不钉)。
pub(super) fn config_with_model(mut cfg: Value, model: &str) -> Value {
    let model = model.trim();
    if model.is_empty() {
        return cfg;
    }
    if !cfg.is_object() {
        cfg = json!({});
    }
    let obj = cfg.as_object_mut().unwrap();
    let env = obj.entry("env".to_string()).or_insert_with(|| json!({}));
    if !env.is_object() {
        *env = json!({});
    }
    let env = env.as_object_mut().unwrap();
    for k in MODEL_ENV_KEYS {
        env.insert((*k).to_string(), Value::String(model.to_string()));
    }
    cfg
}

/// OpenAI 协议供应商未钉模型时的默认模型名。
pub(super) const DEFAULT_OPENAI_MODEL: &str = "gpt5.6-sol";

/// 本地路由配置: 把 base_url 指到本地路由的 `/p/{id}` 前缀(路由按 id 实时解析该家上游,
/// 每对话可并发指向不同家而不串台), 钉四档模型(含小任务档), AUTH_TOKEN 给个占位串
/// (路由不看它 —— ChatGPT 认 ~/.codex/auth.json, OpenAI 协议认 store 里该家的 key),
/// 让 claude 愿意发请求。
pub(super) fn local_route_config(port: u16, id: &str, model: &str) -> Value {
    let mut env = Map::new();
    env.insert(
        "ANTHROPIC_BASE_URL".into(),
        Value::String(format!("http://127.0.0.1:{port}/p/{id}")),
    );
    env.insert(
        "ANTHROPIC_AUTH_TOKEN".into(),
        Value::String(LOCAL_ROUTE_TOKEN.into()),
    );
    for k in MODEL_ENV_KEYS {
        env.insert((*k).into(), Value::String(model.into()));
    }
    env.insert(
        "ANTHROPIC_SMALL_FAST_MODEL".into(),
        Value::String(model.into()),
    );
    json!({ "env": Value::Object(env) })
}

/// 供应商钉选的模型名(ANTHROPIC_MODEL), 空则回落 OpenAI 默认。
pub(super) fn pinned_model_or_default(cfg: &Value) -> String {
    let m = cfg_env_str(cfg, "ANTHROPIC_MODEL");
    let m = m.trim();
    if m.is_empty() {
        DEFAULT_OPENAI_MODEL.to_string()
    } else {
        m.to_string()
    }
}

/// 本地路由的上游目标 —— codex_proxy 按请求路径里的供应商 id 实时解析(改 key/换模型
/// 立即生效, 不用重启会话), 这是 cc-switch 式「本地转发热切换」的根。
pub enum RouteTarget {
    /// ChatGPT 订阅(Responses 协议, 鉴权走 ~/.codex/auth.json)
    Chatgpt,
    /// OpenAI 兼容 chat/completions(鉴权走该供应商存的 API key)
    OpenAiCompat {
        base_url: String,
        api_key: String,
        model: String,
    },
    /// Anthropic 兼容上游 —— 本地路由总开关下的纯透传(不翻译, 只按 token_field
    /// 实时注入鉴权头): ANTHROPIC_API_KEY → x-api-key, 其余 → Authorization Bearer。
    AnthropicCompat {
        base_url: String,
        api_key: String,
        token_field: String,
    },
}

/// 按供应商 id 解析路由目标。空 id / "codex" → ChatGPT(兼容旧版裸端口 base_url)。
pub fn route_target(id: &str) -> Result<RouteTarget, String> {
    if id.is_empty() || id == "codex" {
        return Ok(RouteTarget::Chatgpt);
    }
    let store = STORE.read().clone();
    let views = build_views(&store);
    let v = views
        .iter()
        .find(|v| v.id == id)
        .ok_or_else(|| format!("未知路由目标: {id}"))?;
    if v.kind == "codex" {
        return Ok(RouteTarget::Chatgpt);
    }
    if v.protocol == "openai" {
        let key = v.auth_token.trim();
        if key.is_empty() {
            return Err(format!("供应商「{}」未配置 API Key", v.name));
        }
        let base = normalize_url(&v.base_url);
        if base.is_empty() {
            return Err(format!("供应商「{}」未配置请求地址", v.name));
        }
        return Ok(RouteTarget::OpenAiCompat {
            base_url: base,
            api_key: key.to_string(),
            model: pinned_model_or_default(&v.settings_config),
        });
    }
    // Anthropic 兼容(key/custom)→ 透传目标。不看 route_local 开关: 只要配置指到了
    // /p/{id}(开关开过 / 旧配置残留), 就该被正确转发, 比拒接更稳。
    if v.protocol.is_empty() && (v.kind == "key" || v.kind == "custom") {
        let key = v.auth_token.trim();
        if key.is_empty() {
            return Err(format!("供应商「{}」未配置 API Key", v.name));
        }
        let base = normalize_url(&v.base_url);
        if base.is_empty() {
            return Err(format!("供应商「{}」未配置请求地址", v.name));
        }
        return Ok(RouteTarget::AnthropicCompat {
            base_url: base,
            api_key: key.to_string(),
            token_field: if v.token_field.is_empty() {
                DEFAULT_TOKEN_FIELD.to_string()
            } else {
                v.token_field.clone()
            },
        });
    }
    Err(format!("供应商「{}」不经本地路由", v.name))
}


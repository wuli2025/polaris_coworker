use super::*;
pub(super) fn default_config(base: &str, token_field: &str, token: &str) -> Value {
    let mut env = Map::new();
    let base = base.trim();
    if !base.is_empty() {
        env.insert("ANTHROPIC_BASE_URL".into(), Value::String(base.into()));
    }
    let token = token.trim();
    if !token.is_empty() {
        let field = if token_field.is_empty() {
            DEFAULT_TOKEN_FIELD
        } else {
            token_field
        };
        env.insert(field.into(), Value::String(token.into()));
    }
    json!({ "env": Value::Object(env) })
}

pub(super) fn cfg_env_str(cfg: &Value, key: &str) -> String {
    cfg.get("env")
        .and_then(|e| e.get(key))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

// ─────────────── 密钥掩码(provider_list 出口打码 / provider_save 入口回填)───────────────
//
// 为什么必须打码:`provider_list` 原样吐 `authToken` **和** `settingsConfig.env` 里的真 key。
// 它在 server 壳(Docker/NAS)是**全量命令面**的一员(apihub.rs 的 dispatch_server),
// 而 NAS 版默认免口令、局域网谁都够得着 —— 一条 `{"cmd":"provider_list"}` 就把用户所有
// 上游 API Key 明文取走。手机数据面(dispatch_desktop)早就手工脱敏过一版, 但那只挡住了
// 一个壳; 生图坞(image_provider_list 只给 hasKey)和感官坞(sense.rs 只给 mask_key)也
// 早就是脱敏的 —— 唯独聊天供应商坞这条一直在裸奔。
//
// 为什么不能简单地把字段剥掉:前端**靠这个值回填编辑框**(AddProviderModal 的 apiKey 与
// configJson), 而 `provider_save` 是**整条覆盖写** —— 剥成空字符串的话, 用户只是进去改个
// 模型名再点保存, key 就被那份空配置抹掉了。所以走掩码 + 回填:
//   出口 `mask_secret` 把值换成 `••••••••尾4`;
//   入口 `unmask_secrets` 看到带 `•` 的值就从库里把原值取回来。
// 判据是「值里含 U+2022」—— 这个字符在任何真实 API key 里都不可能出现。

/// 掩码用的圆点(U+2022)。真 key 一定不含它, 故可当「这是掩码、别当真值存」的判据。
pub(super) const MASK_BULLET: char = '•';

/// 除该家自己的 `token_field` 外, env 里还可能装密钥的字段(打码/回填都要覆盖到)。
pub(super) const SECRET_ENV_KEYS: &[&str] = &[
    DEFAULT_TOKEN_FIELD,
    API_KEY_FIELD,
    // Bedrock 两个预设把凭据放在 AWS 那套变量里
    "AWS_SECRET_ACCESS_KEY",
    "AWS_ACCESS_KEY_ID",
    "AWS_BEARER_TOKEN_BEDROCK",
];

/// 打码:留尾 4 位便于用户认出「这是哪把 key」, 其余一律圆点。
/// 空值仍回空 —— 「没配 key」和「配了但不给看」必须能区分, 否则 hasKey 就白给了。
/// 短到 ≤4 位的值不留尾巴(留了等于全给)。
pub(super) fn mask_secret(v: &str) -> String {
    let t = v.trim();
    if t.is_empty() {
        return String::new();
    }
    // 本地路由占位串不是秘密, 打码反而会让「借 key 侧按内容判占位」的逻辑看不懂它。
    if t == LOCAL_ROUTE_TOKEN {
        return t.to_string();
    }
    let chars: Vec<char> = t.chars().collect();
    if chars.len() <= 4 {
        return "•".repeat(chars.len());
    }
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("••••••••{tail}")
}

/// 是否是掩码值(而非用户真填的新 key)。
pub(super) fn is_masked(v: &str) -> bool {
    v.contains(MASK_BULLET)
}

/// 把一份视图里的密钥全部换成掩码(auth_token 与 settings_config.env 双处)。
pub(super) fn mask_view(v: &mut ProviderView) {
    v.auth_token = mask_secret(&v.auth_token);
    let Some(env) = v
        .settings_config
        .get_mut("env")
        .and_then(|e| e.as_object_mut())
    else {
        return;
    };
    for key in SECRET_ENV_KEYS.iter().copied().chain([v.token_field.as_str()]) {
        if let Some(Value::String(s)) = env.get_mut(key) {
            *s = mask_secret(s);
        }
    }
}

/// 入口回填:前端回传的 env 里凡是掩码值, 都从库里取回原值。
///
/// 取回策略两级 —— ①同字段名直接取(绝大多数情况);②字段名对不上时(用户在弹窗里把
/// `ANTHROPIC_AUTH_TOKEN` 切成了 `ANTHROPIC_API_KEY`, 前端会把值搬到新字段), 在旧配置的
/// 全部密钥字段里找**掩码相同**的那个值搬过来。都找不到就把这个字段删掉 ——
/// 宁可让用户重填一次, 也绝不把一串圆点当 key 存进去发给上游。
///
/// 注意只处理「带 `•` 的值」:用户把输入框清空时前端是**删掉**该 env 字段, 那条路径不经过
/// 这里, 于是「清空 key」照旧生效(不会被回填复活)。
pub(super) fn unmask_secrets(mut cfg: Value, id: &str, token_field: &str) -> Value {
    let old = build_views(&STORE.read()).into_iter().find(|v| v.id == id)
        .map(|v| (v.settings_config, v.token_field));
    let Some((old_cfg, old_field)) = old else {
        // 新建的供应商没有旧值可回填:掩码值一律剔除(不可能是真 key)。
        if let Some(env) = cfg.get_mut("env").and_then(|e| e.as_object_mut()) {
            env.retain(|_, v| !v.as_str().is_some_and(is_masked));
        }
        return cfg;
    };
    let candidates: Vec<String> = SECRET_ENV_KEYS
        .iter()
        .map(|s| s.to_string())
        .chain([old_field, token_field.to_string()])
        .collect();
    let Some(env) = cfg.get_mut("env").and_then(|e| e.as_object_mut()) else {
        return cfg;
    };
    let masked_keys: Vec<String> = env
        .iter()
        .filter(|(_, v)| v.as_str().is_some_and(is_masked))
        .map(|(k, _)| k.clone())
        .collect();
    for k in masked_keys {
        let masked = env.get(&k).and_then(|v| v.as_str()).unwrap_or("").to_string();
        // ① 同名字段
        let same = cfg_env_str(&old_cfg, &k);
        let restored = if !same.trim().is_empty() {
            Some(same)
        } else {
            // ② 字段改过名 → 按掩码相同认领旧值
            candidates.iter().find_map(|c| {
                let v = cfg_env_str(&old_cfg, c);
                (!v.trim().is_empty() && mask_secret(&v) == masked).then_some(v)
            })
        };
        match restored {
            Some(real) => {
                env.insert(k, Value::String(real));
            }
            None => {
                env.remove(&k);
            }
        }
    }
    cfg
}

// ───────────────────────── 视图模型 ─────────────────────────


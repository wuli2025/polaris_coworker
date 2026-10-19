use super::*;
/// 还原构建期注入的「粉丝福利」MiniMax key。
/// 二进制内为 XOR 混淆字节, 此处解出明文; 未注入(本地 dev 构建)时返回空串。
/// 提醒: 客户端解密逻辑随包一起分发, 混淆只是延缓提取, 不构成真正保护。
pub(super) fn gift_minimax_key() -> String {
    if GIFT_MINIMAX_OBF.is_empty() || GIFT_MINIMAX_PAD.is_empty() {
        return String::new();
    }
    let bytes: Vec<u8> = GIFT_MINIMAX_OBF
        .iter()
        .enumerate()
        .map(|(i, b)| b ^ GIFT_MINIMAX_PAD[i % GIFT_MINIMAX_PAD.len()])
        .collect();
    String::from_utf8(bytes).unwrap_or_default()
}

/// 供 Polaris 内部小调用(如「语音整形」)借用的 MiniMax key:优先取用户已在坞里
/// 自存的 minimax / minimax-en 供应商 key, 否则回落到构建期注入的「粉丝福利」key。
/// 都没有则返回空串。让语音整形等功能即便用户没单独填 key 也能开箱用上福利额度。
pub fn minimax_borrow_key() -> String {
    {
        let store = hydrated_store();
        for id in ["minimax", "minimax-en"] {
            if let Some(it) = store.items.iter().find(|i| i.id == id) {
                for field in [DEFAULT_TOKEN_FIELD, API_KEY_FIELD] {
                    let tok = cfg_env_str(&it.settings_config, field);
                    if !tok.trim().is_empty() {
                        return tok.trim().to_string();
                    }
                }
            }
        }
    }
    gift_minimax_key()
}

/// **生图**专用借 key:只借用户自己在坞里存的 minimax / minimax-en(Coding Plan)key,
/// **不**回落粉丝福利 key —— 生图按张烧额度, 共享福利 key 扛不住, 只留给语音整形这类小调用。
/// 实测(2026-07-21): `sk-cp-` 前缀的 Coding Plan key 可直接调 `/v1/image_generation`。
/// 回 (key, 是否国际站) —— 国际站 key 须打 api.minimax.io, 国内站打 api.minimaxi.com,
/// 打错主站会 401, 由调用方按此选 endpoint。
pub fn minimax_borrow_key_for_image() -> Option<(String, bool)> {
    fn pick(st: &Store) -> Option<(String, bool)> {
        for id in ["minimax", "minimax-en"] {
            if let Some(it) = st.items.iter().find(|i| i.id == id) {
                for field in [DEFAULT_TOKEN_FIELD, API_KEY_FIELD] {
                    let tok = cfg_env_str(&it.settings_config, field);
                    if !tok.trim().is_empty() {
                        return Some((tok.trim().to_string(), id == "minimax-en"));
                    }
                }
            }
        }
        None
    }
    // 壳内(Tauri/server)STORE 由 init() 装载, 直接读内存。但对话里的生图跑在
    // `polaris-forge image` **子进程**里 —— 没有 AppHandle 不走 init, STORE 恒空;
    // 以 STORE_PATH 是否已设判「未初始化」, 是则从磁盘只读解析 providers.json,
    // 否则聊天出图的借用链路在子进程里永远断(实测就是这么断的)。
    if !STORE_PATH.read().as_os_str().is_empty() {
        return pick(&hydrated_store());
    }
    let user = UserDirs::new()?;
    let path = user
        .home_dir()
        .join("Polaris")
        .join("data")
        .join("providers.json");
    let txt = fs::read_to_string(path).ok()?;
    let mut st: Store = serde_json::from_str(&txt).ok()?;
    hydrate(&mut st, &serde_json::from_str::<Value>(&txt).ok()?);
    pick(&st)
}

/// **火山方舟**系聊天供应商的 id, 按优先级排 —— 「火山方舟 Agentplan」摆第一位,
/// 它就是用户口里的「我们火山的 agentplan」。BytePlus 是同一套方舟的海外站。
pub(super) const VOLC_ARK_IDS: &[&str] = &["agentplan", "doubaoseed", "byteplus"];

/// **生图**专用借 key(火山方舟):借用户已在聊天坞里配好的方舟 key, 拼出方舟的生图端点。
///
/// 与 MiniMax 那条借用同一个理由 —— 用户已经为方舟付过费/配过 key, 没道理再让他去生图坞
/// 重填一遍。回 `(key, endpoint)`: endpoint 由该家 base_url 的**主机名**推出, 因为方舟
/// 国内站(ark.cn-beijing.volces.com)与海外站(ark.ap-southeast.bytepluses.com)是两套域名,
/// 拿错域名必 401;而 base_url 的路径部分(`/api/coding` 之类)是聊天套餐专用前缀,
/// **生图要走 `/api/v3`**, 不能照抄。
pub fn volc_ark_borrow_key_for_image() -> Option<(String, String)> {
    let (key, host) = volc_ark_borrow()?;
    Some((key, format!("https://{host}/api/v3/images/generations")))
}

/// 借用聊天坞里配好的**火山方舟** key, 回 `(key, 主机名)`。
///
/// 生图(images/generations)与语音识别(chat/completions 的 input_audio)都用它 ——
/// 用户已经为方舟付过费/配过 key, 没道理再让他去别处重填一遍。
/// 只回主机名不回完整地址, 因为方舟国内站(ark.cn-beijing.volces.com)与海外站
/// (ark.ap-southeast.bytepluses.com)是两套域名(拿错必 401), 而 base_url 的路径部分
/// (`/api/coding` 之类)是**聊天套餐专用前缀**, 生图/转写都得走 `/api/v3`, 不能照抄。
pub fn volc_ark_borrow() -> Option<(String, String)> {
    fn pick(st: &Store) -> Option<(String, String)> {
        for id in VOLC_ARK_IDS {
            let Some(it) = st.items.iter().find(|i| &i.id == id) else {
                continue;
            };
            let mut key = String::new();
            for field in [DEFAULT_TOKEN_FIELD, API_KEY_FIELD] {
                let tok = cfg_env_str(&it.settings_config, field);
                if !tok.trim().is_empty() {
                    key = tok.trim().to_string();
                    break;
                }
            }
            // 「本地路由」模式下这家的 base_url 会被改写成 127.0.0.1 的 /p/{id}, token 是
            // 占位串 —— 拿它去打方舟必失败, 而且一旦返回 Some 就把整条借用链堵死
            // (火山优先 → MiniMax 也轮不到)。开关关掉后旧配置也可能残留, 所以按内容判而非按开关判。
            if key.is_empty() || key == LOCAL_ROUTE_TOKEN {
                continue;
            }
            let host = ark_host(&cfg_env_str(&it.settings_config, "ANTHROPIC_BASE_URL"), id);
            return Some((key, host));
        }
        None
    }
    // 与 minimax_borrow_key_for_image 同款双路读取:壳内读内存 STORE, 生图子进程
    // (polaris-forge image, 没有 AppHandle 不走 init, STORE 恒空)则直接读磁盘,
    // 否则「对话里出图」这条最常用的链路借不到 key。
    if !STORE_PATH.read().as_os_str().is_empty() {
        return pick(&hydrated_store());
    }
    let user = UserDirs::new()?;
    let path = user
        .home_dir()
        .join("Polaris")
        .join("data")
        .join("providers.json");
    let txt = fs::read_to_string(path).ok()?;
    let mut st: Store = serde_json::from_str(&txt).ok()?;
    hydrate(&mut st, &serde_json::from_str::<Value>(&txt).ok()?);
    pick(&st)
}

/// 从已配的 base_url 里取方舟主机名; 取不到就按 id 回默认站点。
///
/// 「取不到」包括三种:清空过 / 含 `${}` 变量 / **指向本地路由的回环地址** ——
/// 最后这种最阴:本地路由开关开过(或旧配置残留)时 base_url 是
/// `http://127.0.0.1:{port}/p/agentplan`, 照抄主机名就会拼出 `https://127.0.0.1:{port}/api/v3/...`
/// (在 http 端口上说 TLS), 生图与语音转写双双打空炮。一律回落到该家的真站点。
pub(super) fn ark_host(base_url: &str, id: &str) -> String {
    let b = base_url.trim();
    if let Some(rest) = b
        .strip_prefix("https://")
        .or_else(|| b.strip_prefix("http://"))
    {
        let host = rest.split('/').next().unwrap_or("").trim();
        let bare = host.split(':').next().unwrap_or(host);
        let loopback = matches!(bare, "127.0.0.1" | "localhost" | "::1" | "0.0.0.0");
        if !host.is_empty() && !host.contains('$') && !loopback {
            return host.to_string();
        }
    }
    if id == "byteplus" {
        "ark.ap-southeast.bytepluses.com".to_string()
    } else {
        "ark.cn-beijing.volces.com".to_string()
    }
}

/// 还原构建期注入的「免费额度赠送」Kimi For Coding token(XOR 混淆, 见 build.rs)。
/// 与 MiniMax 同 —— 仅从环境变量注入(CI secret POLARIS_GIFT_KIMI_KEY);未注入
/// (本地 dev / 无 secret 构建)时返回空串, seed_gift_kimi 见空即跳过, 不种子。
pub(super) fn gift_kimi_key() -> String {
    if GIFT_KIMI_OBF.is_empty() || GIFT_KIMI_PAD.is_empty() {
        return String::new();
    }
    let bytes: Vec<u8> = GIFT_KIMI_OBF
        .iter()
        .enumerate()
        .map(|(i, b)| b ^ GIFT_KIMI_PAD[i % GIFT_KIMI_PAD.len()])
        .collect();
    String::from_utf8(bytes).unwrap_or_default()
}

/// Legacy Kimi gift support; absent build credentials leave the store unchanged.
pub(super) fn seed_gift_kimi(store: &mut Store, data_dir: &Path) -> bool {
    let key = gift_kimi_key();
    if key.is_empty() {
        return false;
    }
    let marker = data_dir.join(".gift_kimi_seeded");
    if marker.exists() {
        return false;
    }
    let _ = fs::write(&marker, b"seeded\n");

    // 用户已自配 kimi-for-coding 则不覆盖。
    if store.items.iter().any(|i| i.id == "kimi-for-coding") {
        return false;
    }
    // 钉 kimi-for-coding(K2.7 Code):四档全设成同一 model id, 后台小任务也走它,
    // 不回落 Claude 默认 haiku 名被网关当未知模型(与 MiniMax 同理)。
    let cfg = config_with_model(
        default_config("https://api.kimi.com/coding/", DEFAULT_TOKEN_FIELD, &key),
        "kimi-for-coding",
    );
    store.items.push(StoredProvider {
        id: "kimi-for-coding".to_string(),
        name: "Kimi For Coding".to_string(),
        note: "免费额度赠送 · 开箱即用 · K2.7 Code".to_string(),
        website_url: "https://www.kimi.com/code".to_string(),
        token_field: DEFAULT_TOKEN_FIELD.to_string(),
        protocol: String::new(),
        settings_config: cfg,
    });
    true
}


fn hydrate(store: &mut Store, root: &Value) {
    for item in &mut store.items {
        let id = pi_translate::credential_id(&item.id);
        let field = if item.token_field.is_empty() { DEFAULT_TOKEN_FIELD } else { &item.token_field };
        if let Some(key) = root["credentials"][id]["key"].as_str() {
            if !item.settings_config.is_object() { item.settings_config = json!({}); }
            if !item.settings_config["env"].is_object() { item.settings_config["env"] = json!({}); }
            item.settings_config["env"][field] = json!(key);
        }
    }
}
fn hydrated_store() -> Store {
    let mut snapshot = STORE.read().clone();
    if let Some(path) = shared_path() {
        if let Ok(root) = credential_file::read_root(&path) { hydrate(&mut snapshot, &root); }
    }
    snapshot
}

use super::*;
#[cfg_attr(feature = "desktop", tauri::command(async))]
pub fn provider_switch(id: String) -> Result<String, String> {
    let store = STORE.read().clone();
    let views = build_views(&store);
    let v = views
        .iter()
        .find(|v| v.id == id)
        .ok_or_else(|| format!("供应商不存在: {id}"))?;

    if pi_translate::is_pi() {
        {
            let _mutation = PROVIDER_MUTATION.lock();
            STORE.write().current_id = id.clone();
            selection::changed();
        }
        persist_checked()?;
        crate::chat::backend::selected().map_err(|_| "AgentBackendUnavailable")?
            .provider_apply(&id).map_err(|e| e.to_string())?;
        return Ok(id);
    }
    // Legacy process env/settings mutations still serialize independently.
    static LEGACY_SWITCH: Lazy<parking_lot::Mutex<()>> = Lazy::new(|| parking_lot::Mutex::new(()));
    let _legacy = LEGACY_SWITCH.lock();
    let cfg = cfg_for_view(v, store.route_local)?;
    // 联动才碰全局 settings.json; 隔离只走进程 env, 外部 CLI 原封不动 ——
    // 但顺手做一次残留体检: 全局若还躺着我们(旧版/联动时代)写的受管键, 先清掉,
    // 用户不用重启 Polaris 外部 CLI 就立即恢复干净。
    if store.link_global {
        apply_settings_config(&cfg)?;
    } else {
        purge_global_residue(&views, &store);
    }
    apply_process_env(&cfg);

    {
        let _mutation = PROVIDER_MUTATION.lock();
        STORE.write().current_id = id.clone();
        selection::changed();
    }
    persist();
    Ok(id)
}

#[cfg_attr(feature = "desktop", tauri::command(async))]
pub fn provider_set_link_mode(link: bool) -> Result<bool, String> {
    if pi_translate::is_pi() { return Err("ProviderLinkModeUnsupported: Pi 后端不使用 Claude 联动模式".into()); }
    STORE.write().link_global = link;
    persist();

    let store = STORE.read().clone();
    let views = build_views(&store);
    let cur = views.iter().find(|v| v.id == store.current_id);
    if link {
        // 开联动: 当前供应商立刻写入全局, 终端 CLI 即刻跟上。
        if let Some(v) = cur {
            if let Ok(cfg) = cfg_for_view(v, store.route_local) {
                apply_settings_config(&cfg)?;
                apply_process_env(&cfg);
            }
        }
    } else {
        // 关联动(转隔离): 全局退回官方(只清我们的受管键, 其余原样保留),
        // Polaris 自身改用进程 env 维持当前选择 —— 终端立刻恢复干净。
        apply_settings_config(&json!({ "env": {} }))?;
        if let Some(v) = cur {
            if let Ok(cfg) = cfg_for_view(v, store.route_local) {
                apply_process_env(&cfg);
            }
        }
    }
    Ok(link)
}

/// 本地路由总开关(cc-switch 式代理模式)。开 = 当前及后续切换的 Anthropic 兼容供应商
/// 统一改走 127.0.0.1 本地路由透传(热切换: 改 key 即刻生效); 关 = 回到直连。
/// 切换开关立即对「当前供应商」重新生效, 联动模式下全局 settings.json 同步校正。
#[cfg_attr(feature = "desktop", tauri::command(async))]
pub fn provider_set_route_mode(route: bool) -> Result<bool, String> {
    if pi_translate::is_pi() { return Err("ProviderRouteModeUnsupported: Pi 直接连接上游".into()); }
    STORE.write().route_local = route;
    persist();

    let store = STORE.read().clone();
    let views = build_views(&store);
    if let Some(v) = views.iter().find(|v| v.id == store.current_id) {
        if let Ok(cfg) = cfg_for_view(v, store.route_local) {
            if store.link_global {
                apply_settings_config(&cfg)?;
            }
            apply_process_env(&cfg);
        }
    }
    Ok(route)
}

#[cfg_attr(feature = "desktop", tauri::command(async))]
pub fn provider_save(input: ProviderInput) -> Result<String, String> {
    let token_field = input
        .token_field
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_TOKEN_FIELD.to_string());

    // settings_config 兜底为 {env:{}}
    let cfg = if input.settings_config.is_object() {
        input.settings_config.clone()
    } else {
        json!({ "env": {} })
    };

    let id = input
        .id
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("custom-{}", now_ms()));

    // 掩码回填:provider_list 出口已把密钥打成 `••••••••尾4`, 而这里是**整条覆盖写** ——
    // 用户只改了个模型名就点保存时, 回传的正是那串圆点, 不还原就等于把 key 抹成一串没用的
    // 圆点(拿去打上游必 401)。清空 key 走的是「删掉该 env 字段」那条路, 不受影响。
    // A masked value means "keep current credential", not "write this old
    // snapshot back". Login/refresh may have rotated it before we get the lock.
    let preserve_key = cfg_env_str(&cfg, &token_field).contains(MASK_BULLET);
    let mut cfg = unmask_secrets(cfg, &id, &token_field);
    if preserve_key && shared_path().is_some() {
        if let Some(env) = cfg.get_mut("env").and_then(Value::as_object_mut) {
            env.remove(&token_field);
            env.remove(DEFAULT_TOKEN_FIELD);
            env.remove(API_KEY_FIELD);
        }
    }

    // Pi 支持的协议名在具名映射表内；未知协议报错，不能静默降为 Anthropic。
    let protocol = match input.protocol.as_deref().map(str::trim) {
        None | Some("") => String::new(),
        Some(p) if pi_translate::PROTOCOL_APIS.iter().any(|(name, _)| *name == p) => p.into(),
        _ => return Err("ProviderProtocolUnsupported".into()),
    };

    let clear_key = !preserve_key && cfg_env_str(&cfg, &token_field).is_empty()
        && credential_file::read(pi_translate::credential_id(&id))?
            .is_some_and(|c| c["type"] == "api_key");
    let item = StoredProvider {
        id: id.clone(),
        name: input.name.trim().to_string(),
        note: input.note.trim().to_string(),
        website_url: normalize_url(&input.website_url),
        token_field,
        protocol,
        settings_config: cfg,
    };

    let mutation = PROVIDER_MUTATION.lock();
    let mut store = STORE.write();
    if let Some(existing) = store.items.iter_mut().find(|i| i.id == id) {
        *existing = item;
    } else {
        store.items.push(item);
    }
    selection::changed();
    drop(store);
    drop(mutation);
    persist_without(clear_key.then_some(id.as_str()))?;
    Ok(id)
}

#[cfg_attr(feature = "desktop", tauri::command(async))]
pub fn provider_delete(id: String) -> Result<(), String> {
    let mutation = PROVIDER_MUTATION.lock();
    let mut store = STORE.write();
    store.items.retain(|i| i.id != id);
    if store.current_id == id {
        store.current_id = "claude-official".to_string();
    }
    selection::changed();
    drop(store);
    drop(mutation);
    persist_without(Some(&id))?;
    Ok(())
}


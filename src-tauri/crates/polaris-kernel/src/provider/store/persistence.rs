use super::*;
pub fn init(_app: &AppHandle) -> Result<()> {
    // 故障转移事件通道: 失败上报点在无 app 句柄的 TCP 线程里, 这里存一份全局。
    *FAILOVER_APP.write() = Some(_app.clone());
    let user = UserDirs::new().ok_or_else(|| anyhow::anyhow!("no user dir"))?;
    let dir = user.home_dir().join("Polaris").join("data");
    fs::create_dir_all(&dir)?;
    let path = dir.join("providers.json");
    *STORE_PATH.write() = path.clone();

    let mut store: Store = if path.exists() {
        let txt = fs::read_to_string(&path).unwrap_or_default();
        match serde_json::from_str(&txt) {
            Ok(s) => s,
            Err(_) => {
                // 解析失败别静默 default —— 那会让用户所有已存供应商/API key 凭空消失。
                // 先把损坏文件留底, 用户仍可手工抢救, 再回落空 store。
                if !txt.trim().is_empty() {
                    let mut bak = path.as_os_str().to_owned();
                    bak.push(".corrupt.bak");
                    let _ = fs::copy(&path, PathBuf::from(bak));
                }
                Store::default()
            }
        }
    } else {
        Store::default()
    };

    // 一次性迁移旧 custom / keys → items
    let mut migrated = false;
    let legacy_custom = std::mem::take(&mut store.custom);
    let legacy_keys = std::mem::take(&mut store.keys);
    for c in legacy_custom {
        if store.items.iter().any(|i| i.id == c.id) {
            continue;
        }
        let field = if c.token_field.is_empty() {
            DEFAULT_TOKEN_FIELD.to_string()
        } else {
            c.token_field.clone()
        };
        store.items.push(StoredProvider {
            id: c.id,
            name: c.name,
            note: String::new(),
            website_url: String::new(),
            token_field: field.clone(),
            protocol: String::new(),
            settings_config: default_config(&c.base_url, &field, &c.auth_token),
        });
        migrated = true;
    }
    for (pid, k) in legacy_keys {
        if store.items.iter().any(|i| i.id == pid) {
            continue;
        }
        if let Some(p) = preset_by_id(&pid) {
            let field = if k.token_field.is_empty() {
                p.token_field.to_string()
            } else {
                k.token_field.clone()
            };
            store.items.push(StoredProvider {
                id: pid.clone(),
                name: p.name.to_string(),
                note: String::new(),
                website_url: String::new(),
                token_field: field.clone(),
                protocol: String::new(),
                settings_config: default_config(p.base_url, &field, &k.auth_token),
            });
            migrated = true;
        }
    }

    // 首启一次性种「粉丝福利」MiniMax + 「免费额度赠送」Kimi For Coding(含内置 key)。
    let gifted_minimax = seed_gift_minimax(&mut store, &dir);
    let gifted_kimi = seed_gift_kimi(&mut store, &dir);

    if store.current_id.is_empty() { store.current_id = "claude-official".into(); }
    *STORE.write() = store;
    let _ = (migrated, gifted_minimax, gifted_kimi);
    persist_checked().map_err(anyhow::Error::msg)?;
    if pi_translate::is_pi() { return Ok(()); }

    {
        let store = STORE.read().clone();
        let views = build_views(&store);
        if store.link_global {
            // 联动: 若上次退出时正走本地路由(Codex / OpenAI 协议 / 路由总开关), 重启后
            // 端口可能变 → 重新拉起路由并校正 ANTHROPIC_BASE_URL, 否则 settings.json
            // 残留旧端口 claude 连不上。直连供应商的 settings.json 本来就是对的, 不重写。
            let cur = detect_current(&views, &store);
            if let Some(v) = views.iter().find(|v| v.id == cur) {
                if let Ok(cfg) = cfg_for_view(v, store.route_local) {
                    if cfg_env_str(&cfg, "ANTHROPIC_BASE_URL").starts_with("http://127.0.0.1:") {
                        let _ = apply_settings_config(&cfg);
                        apply_process_env(&cfg);
                    }
                }
            }
        } else {
            // 隔离(默认):
            // ① 净化 —— 联动时代(或旧版本)写进全局 settings.json 的受管键还躺在那里
            //    污染外部 CLI。旧规则只认「全局 base_url == 当前供应商」, 用户切回官方后
            //    current 的 base 变空, 残留永远匹配不上、永远清不掉(实测就是「切一次
            //    MiniMax 就再也切不回来」)。证据判定改走 global_env_is_ours 四级证据。
            purge_global_residue(&views, &store);
            // ② 重启后进程 env 是空的, 把当前供应商重新作用上去, 否则 Polaris 内的
            //    选择一重启就回落官方。配置不全(如 key 被删)就静默跳过 = 官方。
            if let Some(v) = views.iter().find(|v| v.id == store.current_id) {
                if let Ok(cfg) = cfg_for_view(v, store.route_local) {
                    apply_process_env(&cfg);
                }
            }
        }
    }
    Ok(())
}

/// 原子落盘: 先写同目录临时文件, 再 rename 覆盖目标。
/// rename 在同一文件系统内是原子的 (Windows 的 `fs::rename` 用 MoveFileExW+REPLACE_EXISTING),
/// 故进程在写一半时崩溃/断电只会留下 `.polaris.tmp`, 目标文件要么旧内容要么新内容,
/// 绝不会被截成半截 JSON —— 这是「torn write 破坏 claude 配置 / 静默清空 API key」的根治。
pub(crate) fn atomic_write(path: &Path, contents: &str) -> std::io::Result<()> {
    // tmp 名带进程内唯一序号:固定名下两个并发写者(如 codex 刷新回写 vs 重新授权落盘)
    // 会互相截断对方的 tmp,先 rename 的一方可能把「对方的半截内容」提交成目标文件。
    // 唯一名让各写各的,rename 仍原子,后完成者整体覆盖(last-wins,不会撕裂)。
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".polaris.{}.{}.tmp", std::process::id(), n));
    let tmp = PathBuf::from(tmp);
    // 写完必须 sync_all 再 rename: rename 只保证目录项切换原子, 不保证 tmp 数据已刷盘。
    // 断电时 rename(元数据)可能先于数据落盘 → 目标变 0 字节/半截, 「绝不残缺」失效。
    // (rename 后不 fsync 父目录: 最坏回退到旧文件, 仍是完整 JSON, 可接受。)
    {
        use std::io::Write;
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)] {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut f = options.open(&tmp)?;
        f.write_all(contents.as_bytes())?;
        f.sync_all()?;
    }
    // 收紧权限:providers.json 内含明文 API key —— unix 上设 0o600(仅属主可读写),防同机其它
    // 用户、或文件被同步/备份进宽权限目录时被顺手读走。在 rename 前设,确保最终文件落地即 600。
    // (Windows 用户配置目录已按用户 ACL 隔离,此处不额外处理。真·防同用户恶意软件仍需 OS 钥匙串
    //  级静态加密 —— 见 security 备忘,作为后续带真机验证的独立改动。)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))?;
    }
    fs::rename(&tmp, path)
}

pub(super) fn persist() {
    if persist_checked().is_err() { eprintln!("[provider] ProviderStoreWriteFailed"); }
}

pub(super) fn persist_checked() -> Result<(), String> { persist_without(None) }

pub(super) fn persist_without(remove: Option<&str>) -> Result<(), String> {
    let Some(path) = shared_path() else { return Ok(()); };
    let _io = IO_LOCK.lock();
    let _file = credential_file::FileLock::acquire(&path)?;
    let disk = credential_file::read_root(&path)?;
    let mut credentials = disk.get("credentials").cloned().unwrap_or_else(|| json!({}));
    let entries = credentials.as_object_mut().ok_or("CredentialStoreInvalid")?;
    if let Some(id) = remove { entries.remove(pi_translate::credential_id(id)); }
    let mut store = STORE.write();
    let mut candidate = store.clone();
    move_credentials(&mut candidate, entries);
    let mut value = serde_json::to_value(&candidate).map_err(|_| "ProviderStoreInvalid")?;
    value["credentials"] = credentials;
    atomic_write(&path, &serde_json::to_string_pretty(&value).map_err(|_| "ProviderStoreInvalid")?)
        .map_err(|_| "ProviderStoreWriteFailed")?;
    *store = candidate;
    Ok(())
}

//! Mutating device console commands; approval precedes side effects.
use super::*;

/// 改一台设备在**本机**的档位(盘 none/ro/rw、执行 none/ask/allow、自动挂载)。
///
/// 立刻落库(重启仍在),并当场生效:关掉自动挂载/降成 none 会把已挂的盘收回来,
/// 改回来则下一拍对账自动挂上 —— 不必让用户重启应用才看见效果。
#[cfg_attr(feature = "desktop", tauri::command)]
#[allow(non_snake_case)]
pub async fn peer_grant_set(
    nodeId: String,
    fsAccess: Option<String>,
    execAccess: Option<String>,
    autoMount: Option<bool>,
) -> Result<Value, String> {
    security::require_local("change_device_permissions",
        &json!({"nodeId":nodeId,"fsAccess":fsAccess,"execAccess":execAccess,"autoMount":autoMount}))?;
    let g = crate::collab::grants::update(
        &nodeId,
        None,
        fsAccess.as_deref(),
        execAccess.as_deref(),
        autoMount,
        "本机",
    )?;
    if !g.should_mount() {
        drop_peer(&nodeId).await;
    } else {
        // 档位变了(比如 ro→rw)要重挂才生效:writable 是挂载那一刻钉进系统的。
        drop_peer(&nodeId).await;
        let _ = mesh_sync().await;
    }
    Ok(json!({ "ok": true, "effectiveFs": g.effective_fs(), "autoMount": g.auto_mount }))
}

/// 「信任这台设备」:冷静期清零 **+ 升到全权**(顺带把误撤销的恢复回来)。
///
/// 升档这一半是设备码那条路带来的:靠码进来的设备没有冷静期,它的限制就是
/// `fs=ro / exec=none` 这两个档位本身。只清冷静期的话,用户点完按钮什么都不会变。
#[cfg_attr(feature = "desktop", tauri::command)]
#[allow(non_snake_case)]
pub async fn peer_trust(nodeId: String) -> Result<Value, String> {
    security::require_local("change_device_permissions", &json!({"nodeId":nodeId,"trust":true}))?;
    let g = crate::collab::grants::trust_fully(&nodeId, "本机")?;
    // 冷静期里那块盘是钉死只读挂上的,清零后必须重挂才真的可写。
    drop_peer(&nodeId).await;
    let _ = mesh_sync().await;
    Ok(json!({ "ok": true, "effectiveFs": g.effective_fs() }))
}

/// 撤销一台设备在**本机**的准入,并当场收回它的盘与隧道。
///
/// 与云端的 [`mesh_kick`] 是两道独立的闸,丢了电脑应该都点:
/// 云端那道让它拿不到新断言,本机这道让它连断言都递不进来。
#[cfg_attr(feature = "desktop", tauri::command)]
#[allow(non_snake_case)]
pub async fn peer_revoke(nodeId: String) -> Result<Value, String> {
    security::require_local("revoke_other_device", &json!({"nodeId":nodeId}))?;
    crate::collab::grants::revoke(&nodeId, "本机")?;
    drop_peer(&nodeId).await;
    Ok(json!({ "ok": true }))
}

/// 给设备改个人话名字(改的是云端台账,所有设备上都跟着变)。
#[cfg_attr(feature = "desktop", tauri::command)]
#[allow(non_snake_case)]
pub async fn mesh_rename(nodeId: String, name: String) -> Result<Value, String> {
    let Some(c) = cfg() else {
        return Err("本机还没登录".into());
    };
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        agent()?
            .post(&format!("{}/api/mesh/rename", c.url))
            .set("Authorization", &format!("Bearer {}", c.key))
            .send_json(json!({ "nodeId": nodeId, "name": name }))
            .map_err(cloud_err)?;
        Ok(())
    })
    .await
    .map_err(|e| format!("改名任务失败:{e}"))??;
    Ok(json!({ "ok": true }))
}

/// 退网:清掉本机的设备密钥与账号 UID,拆掉所有自动挂上的盘。
/// 云端名册上这台设备仍在(只是不再报到,很快显示离线)—— 要彻底踢掉用 [`mesh_kick`]。
#[cfg_attr(feature = "desktop", tauri::command)]
pub async fn mesh_leave() -> Result<Value, String> {
    security::require_local("remove_remote_storage", &json!({"leave":true}))?;
    // 即使极少见的 SQLite 写入失败，也先把已挂盘和隧道收干净；错误最后如实上抛。
    let clear = crate::collab::db::meta_set_many(&[(K_KEY, ""), (K_UID, "")]);
    member_warn().lock().unwrap_or_else(|error| error.into_inner()).clear();
    let all: Vec<String> = links().lock().unwrap_or_else(|error| error.into_inner()).keys().cloned().collect();
    for n in all {
        drop_peer(&n).await;
    }
    clear?;
    Ok(json!({ "enrolled": false }))
}

/// 把自己账号名下的某台设备移出设备网(丢了电脑的应急操作)。
#[cfg_attr(feature = "desktop", tauri::command)]
#[allow(non_snake_case)]
pub async fn mesh_kick(nodeId: String) -> Result<Value, String> {
    crate::collab::interconnect::revocations::require_fresh(&meta(K_UID))?;
    let Some(c) = cfg() else {
        return Err("本机还没入网".into());
    };
    let nid = nodeId.clone();
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        agent()?
            .post(&format!("{}/api/mesh/revoke", c.url))
            .set("Authorization", &format!("Bearer {}", c.key))
            .send_json(json!({ "nodeId": nid }))
            .map_err(cloud_err)?;
        Ok(())
    })
    .await
    .map_err(|e| format!("吊销任务失败:{e}"))??;
    drop_peer(&nodeId).await;
    Ok(json!({ "ok": true }))
}

/// 丢失设备的一次性安全操作：云端撤 meshKey + 本机撤准入，两条腿无论谁先失败都继续。
/// 返回 partial 供 UI 明确提示并允许重试；两边接口都幂等，重复点击不会恢复任何权限。
#[cfg_attr(feature = "desktop", tauri::command)]
#[allow(non_snake_case)]
pub async fn mesh_device_remove(nodeId: String) -> Result<Value, String> {
    let node_id = nodeId.trim().to_string();
    if node_id.is_empty() {
        return Err("缺 NodeId".into());
    }
    if node_id == my_node_id() {
        return Err("不能从当前设备上移出当前设备；请使用退出登录".into());
    }

    mesh_kick(node_id.clone()).await?;
    let cloud: Result<Value, String> = Ok(json!({"ok":true}));
    let local = crate::collab::grants::revoke(&node_id, "本机");
    // 即便本地 SQLite 极少见地报错，也先拆掉当前内存链路，缩小权限窗口。
    drop_peer(&node_id).await;

    let cloud_ok = cloud.is_ok();
    let local_ok = local.is_ok();
    Ok(json!({
        "ok": cloud_ok && local_ok,
        "partial": cloud_ok != local_ok,
        "cloudRevoked": cloud_ok,
        "localRevoked": local_ok,
        "retryable": !(cloud_ok && local_ok),
        "cloudError": cloud.err().unwrap_or_default(),
        "localError": local.err().unwrap_or_default(),
    }))
}


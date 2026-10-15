//! Peer connection and feature-specific fallback adapters.
use super::*;

#[cfg(feature = "collab-net")]
fn login_peer(port: u16, assertion: &str) -> Result<String, String> {
    let resp = agent()
        .post(&format!(
            "http://127.0.0.1:{port}/api/collab/login_assertion"
        ))
        .send_json(json!({ "assertion": assertion, "deviceId": my_node_id() }))
        .map_err(cloud_err)?;
    let v: Value = resp
        .into_json()
        .map_err(|e| format!("对端返回的不是 JSON:{e}"))?;
    v.get("token")
        .and_then(|x| x.as_str())
        .map(String::from)
        .ok_or_else(|| "对端没返回会话 token".into())
}

/// 同时验证 loopback 隧道与对端 session。`token.is_some()` 只能说明过去登录成功过，
/// 不能证明 tunnel 仍 connected、session 没被撤销；用轻量 `/api/collab/me` 才是事实。
#[cfg(feature = "collab-net")]
fn probe_peer_session(port: u16, token: &str) -> Result<(), String> {
    agent()
        .get(&format!("http://127.0.0.1:{port}/api/collab/me"))
        .set("Authorization", &format!("Bearer {token}"))
        .call()
        .map(|_| ())
        .map_err(cloud_err)
}

// ────────────────────────────── 对账:清单 → 隧道 + 盘符 ──────────────────────────────

/// 确保一台设备的 tunnel、session 与挂载凭据都真实可用；历史 token 只是一条缓存，
/// 每轮对账都要用 tunnel registry + `/api/collab/me` 重新证明，不能拿缓存冒充在线。
#[cfg(feature = "collab-net")]
pub(super) async fn ensure_peer(c: &Cfg, node_id: &str, name: &str, uid: &str) {
    // 首次见到这台同账号设备 → 自动批准(带冷静期);已有契约 = 原样沿用用户调过的档。
    let first = !crate::collab::grants::any_granted().unwrap_or(false);
    let grant = match crate::collab::grants::auto_grant_self(node_id, uid, name, first) {
        Ok((g, _)) => g,
        Err(e) => {
            set_err(node_id, "grant_failed", format!("登记设备契约失败:{e}"));
            return;
        }
    };
    if grant.revoked || !grant.should_mount() {
        // 授权降为不可挂载时要立刻收回已经存在的盘和隧道，不能只阻止下一次新建。
        drop_peer(node_id).await;
        return;
    }
    let ro = grant.effective_fs() == crate::collab::grants::FS_RO;
    let drive_hint = grant.drive_hint.clone();
    let port = port_for(node_id);
    {
        let mut map = links().lock().unwrap_or_else(|error| error.into_inner());
        let link = map.entry(node_id.to_string()).or_insert_with(|| Link {
            name: name.to_string(),
            port,
            token: None,
            phase: "connecting".into(),
            error_code: String::new(),
            last_verified_at: 0,
            err: String::new(),
        });
        link.name = name.to_string();
    }

    // 快路径也必须实探。会话仍有效且 mount 已登记时，仅原位刷新 token，盘符完全不动。
    let cached = links()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .get(node_id)
        .and_then(|l| l.token.clone());
    if let Some(token) = cached {
        #[cfg(feature = "collab-net")]
        let tunnel_ok = crate::collab::tunnel::client_state(port).connected;
        #[cfg(not(feature = "collab-net"))]
        let tunnel_ok = false;
        if tunnel_ok {
            let probe_token = token.clone();
            let probe = tokio::task::spawn_blocking(move || probe_peer_session(port, &probe_token))
                .await
                .unwrap_or_else(|e| Err(format!("探活任务失败:{e}")));
            if probe.is_ok() {
                let sid = source_id(node_id);
                if crate::fsmount::fs_mount_refresh_token(&sid, &token) {
                    let mounted = crate::fsmount::fs_mount_status()
                        .into_iter()
                        .find(|v| v.get("sourceId").and_then(|x| x.as_str()) == Some(sid.as_str()));
                    let mount_ok = mounted
                        .as_ref()
                        .and_then(|v| v.get("ok"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    mark_verified(node_id, if mount_ok { "ready" } else { "mounting" });
                    return;
                }
                // session/tunnel 健康但挂载记录丢了：直接复用已经实探通过的 session。
                // 账号中心临时离线不应阻止一块仍获授权的远程盘恢复。
                match crate::fsmount::fs_mount(
                    sid,
                    name.to_string(),
                    node_id.to_string(),
                    port,
                    token.clone(),
                    Some(ro),
                    Some(drive_hint.clone()),
                )
                .await
                {
                    Ok(v) => {
                        let mount_ok = v.get("ok").and_then(|x| x.as_bool()).unwrap_or(false);
                        mark_verified(node_id, if mount_ok { "ready" } else { "mounting" });
                        if let Some(d) = v["drive"].as_str().filter(|s| !s.is_empty()) {
                            let _ = crate::collab::grants::remember_drive(node_id, d);
                        }
                    }
                    Err(e) => set_err(node_id, "mount_failed", format!("挂盘失败:{e}")),
                }
                return;
            } else if let Err(e) = probe {
                set_err(node_id, "session_probe_failed", e);
            }
        } else {
            let detail = {
                #[cfg(feature = "collab-net")]
                {
                    let s = crate::collab::tunnel::client_state(port);
                    if s.last_error.is_empty() {
                        format!("隧道状态:{}", s.state)
                    } else {
                        s.last_error
                    }
                }
                #[cfg(not(feature = "collab-net"))]
                {
                    "此构建没有 P2P 隧道(collab-net)".to_string()
                }
            };
            set_err(node_id, "tunnel_unhealthy", detail);
        }
        if let Some(l) = links().lock().unwrap_or_else(|error| error.into_inner()).get_mut(node_id) {
            l.token = None;
        }
    }

    set_phase(node_id, "connecting");
    #[cfg(feature = "collab-net")]
    {
        let nid = node_id.to_string();
        let r =
            tokio::task::spawn_blocking(move || crate::collab::tunnel::connect_client(&nid, port))
                .await;
        match r {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                set_err(
                    node_id,
                    "tunnel_connect_failed",
                    format!("隧道建立失败:{e}"),
                );
                return;
            }
            Err(e) => {
                set_err(node_id, "tunnel_task_failed", format!("隧道任务失败:{e}"));
                return;
            }
        }
    }
    #[cfg(not(feature = "collab-net"))]
    {
        set_err(
            node_id,
            "collab_net_missing",
            "此构建没有 P2P 隧道(collab-net),设备网只能看不能连".into(),
        );
        return;
    }

    set_phase(node_id, "authenticating");
    let url = c.url.clone();
    let key = c.key.clone();
    let got = tokio::task::spawn_blocking(move || {
        let a = fresh_assertion(&Cfg { url, key })?;
        login_peer(port, &a)
    })
    .await
    .unwrap_or_else(|e| Err(format!("登录任务失败:{e}")));
    let token = match got {
        Ok(t) => t,
        Err(e) => {
            set_err(node_id, "peer_login_failed", e);
            return;
        }
    };

    set_phase(node_id, "mounting");
    let sid = source_id(node_id);
    if crate::fsmount::fs_mount_refresh_token(&sid, &token) {
        if let Some(l) = links().lock().unwrap_or_else(|error| error.into_inner()).get_mut(node_id) {
            l.token = Some(token);
        }
        mark_verified(node_id, "mounting");
        return;
    }
    match crate::fsmount::fs_mount(
        sid,
        name.to_string(),
        node_id.to_string(),
        port,
        token.clone(),
        Some(ro),
        Some(drive_hint),
    )
    .await
    {
        Ok(v) => {
            if let Some(l) = links().lock().unwrap_or_else(|error| error.into_inner()).get_mut(node_id) {
                l.token = Some(token);
            }
            let mount_ok = v.get("ok").and_then(|x| x.as_bool()).unwrap_or(false);
            mark_verified(node_id, if mount_ok { "ready" } else { "mounting" });
            if let Some(d) = v["drive"].as_str().filter(|s| !s.is_empty()) {
                let _ = crate::collab::grants::remember_drive(node_id, d);
            }
            eprintln!(
                "[mesh] 「{name}」已接入(端口 {port},盘:{}{})",
                v["drive"],
                if ro { ",只读" } else { "" }
            );
        }
        Err(e) => set_err(node_id, "mount_failed", format!("挂盘失败:{e}")),
    }
}

#[cfg(feature = "collab-net")]
fn set_phase(node_id: &str, phase: &str) {
    if let Some(l) = links().lock().unwrap_or_else(|error| error.into_inner()).get_mut(node_id) {
        l.phase = phase.to_string();
    }
}

#[cfg(feature = "collab-net")]
fn mark_verified(node_id: &str, phase: &str) {
    if let Some(l) = links().lock().unwrap_or_else(|error| error.into_inner()).get_mut(node_id) {
        l.phase = phase.to_string();
        l.error_code.clear();
        l.err.clear();
        l.last_verified_at = now_secs();
    }
}

#[cfg(feature = "collab-net")]
fn set_err(node_id: &str, code: &str, e: String) {
    eprintln!("[mesh] {node_id} 接入未成({code}):{e}");
    let mut map = links().lock().unwrap_or_else(|error| error.into_inner());
    if let Some(l) = map.get_mut(node_id) {
        l.phase = "degraded".into();
        l.error_code = code.to_string();
        l.err = e;
    }
}

/// **连对方**:粘一串码就完事 —— 起隧道 → 敲门 → 挂盘。
///
/// 与设备网那条路(同账号自动连)并行不悖:这条用于**别人的**机器,或者
/// 自己的机器但云端暂时联系不上。两条路最后落到同一个地方:一行设备契约 + 一块盘。
#[cfg(feature = "collab-net")]
#[cfg_attr(feature = "desktop", tauri::command)]
pub async fn connect_by_code(code: String) -> Result<Value, String> {
    let (access, node_id) = parse_device_code(&code)?;
    if node_id == my_node_id() {
        return Err("这是你自己这台机器的码 —— 要连的是**对方**那串".into());
    }
    let port = port_for(&node_id);
    links().lock().unwrap_or_else(|error| error.into_inner()).insert(
        node_id.clone(),
        Link {
            name: "对方设备".into(),
            port,
            token: None,
            phase: "connecting".into(),
            error_code: String::new(),
            last_verified_at: 0,
            err: String::new(),
        },
    );

    // ① 隧道(幂等)。
    #[cfg(feature = "collab-net")]
    {
        let nid = node_id.clone();
        let r =
            tokio::task::spawn_blocking(move || crate::collab::tunnel::connect_client(&nid, port))
                .await;
        if let Ok(Err(e)) = r {
            drop_peer(&node_id).await;
            return Err(format!("连不上对方(隧道建立失败):{e}"));
        }
    }
    #[cfg(not(feature = "collab-net"))]
    {
        drop_peer(&node_id).await;
        return Err("此构建没有 P2P 隧道(collab-net),连不了".into());
    }

    // ② 敲门分两步：先取一次性挑战，再用本机 host.key 证明 `me` 真是自己的 NodeId。
    // 只有设备码而没有对应私钥的人，不能冒充台账里已经升过 rw/exec 的老设备。
    let me = my_node_id();
    let my_name_s = my_name();
    let got = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let challenge_resp = agent()
            .post(&format!(
                "http://127.0.0.1:{port}/api/collab/connect/challenge"
            ))
            .send_json(json!({ "nodeId": me.clone() }))
            .map_err(|e| format!("取对方连接挑战失败(对方可能需要升级):{}", cloud_err(e)))?;
        let challenge_body: Value = challenge_resp
            .into_json()
            .map_err(|e| format!("对方连接挑战不是 JSON:{e}"))?;
        let challenge = challenge_body["challenge"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("对方没返回连接挑战 —— 请先升级对方的 Polaris")?;
        let signature = crate::collab::identity::sign_connect_proof(&me, &access, challenge)?;

        let resp = agent()
            .post(&format!("http://127.0.0.1:{port}/api/collab/connect"))
            .send_json(json!({
                "code": access,
                "nodeId": me,
                "challenge": challenge,
                "signature": signature,
                "name": my_name_s,
                "os": my_os(),
                "ver": my_ver(),
            }))
            .map_err(cloud_err)?;
        resp.into_json::<Value>()
            .map_err(|e| format!("对方返回的不是 JSON:{e}"))
    })
    .await
    .unwrap_or_else(|e| Err(format!("敲门任务失败:{e}")));
    let v = match got {
        Ok(v) => v,
        Err(e) => {
            drop_peer(&node_id).await;
            return Err(e);
        }
    };
    let token = v["token"].as_str().unwrap_or_default().to_string();
    if token.is_empty() {
        drop_peer(&node_id).await;
        return Err("对方没给会话令牌 —— 它可能是个还没升级的老版本".into());
    }

    // ③ 挂盘。对方给的是只读就挂只读 —— 挂成读写再让用户撞一鼻子灰是最糟的选择。
    let ro = v["fsAccess"].as_str().unwrap_or("ro") != crate::collab::grants::FS_RW;
    let name = v["peerName"]
        .as_str()
        .filter(|s| !s.is_empty())
        .unwrap_or("对方设备")
        .to_string();
    let mounted = crate::fsmount::fs_mount(
        source_id(&node_id),
        name.clone(),
        node_id.clone(),
        port,
        token.clone(),
        Some(ro),
        None,
    )
    .await;
    {
        let mount_ok = mounted
            .as_ref()
            .ok()
            .and_then(|v| v.get("ok"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let mut map = links().lock().unwrap_or_else(|error| error.into_inner());
        if let Some(l) = map.get_mut(&node_id) {
            l.name = name.clone();
            l.token = Some(token);
            l.phase = if mount_ok {
                "ready"
            } else if mounted.is_ok() {
                "mounting"
            } else {
                "degraded"
            }
            .into();
            l.error_code = if mounted.is_ok() {
                String::new()
            } else {
                "mount_failed".into()
            };
            l.last_verified_at = now_secs();
            l.err = mounted.as_ref().err().cloned().unwrap_or_default();
        }
    }
    Ok(json!({
        "ok": true,
        "nodeId": node_id,
        "name": name,
        "readOnly": ro,
        "note": v["note"],
        "drive": mounted.as_ref().ok().and_then(|x| x["drive"].as_str()).unwrap_or(""),
        "mountError": mounted.err().unwrap_or_default(),
    }))
}


#[cfg(not(feature = "collab-net"))]
pub(super) async fn ensure_peer(_config: &Cfg, node_id: &str, name: &str, _uid: &str) {
    let port = port_for(node_id);
    if let Ok(mut map) = links().lock() {
        map.insert(node_id.into(), Link {
            name: name.into(), port, token: None, phase: "degraded".into(),
            error_code: "collab_net_missing".into(), last_verified_at: 0,
            err: "此构建没有 P2P 隧道(collab-net)".into(),
        });
    }
}

#[cfg(not(feature = "collab-net"))]
#[cfg_attr(feature = "desktop", tauri::command)]
pub async fn connect_by_code(code: String) -> Result<Value, String> {
    parse_device_code(&code)?;
    Err("此构建没有 P2P 隧道(collab-net),连不了".into())
}

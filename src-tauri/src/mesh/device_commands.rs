//! Device inventory and local device-code commands.
use super::*;

// ────────────────────────────── 命令 ──────────────────────────────

/// **设备台账**:一次给全 —— 云端目录(有哪些设备)+ 本机契约(各自什么档)+ 实时链路
/// (连上没、挂成哪个盘)。前端一个请求画完整页,不必自己 join 三份数据。
///
/// 云端拉不到(断网/云机挂了)不算失败:退回只用本机契约与实时链路画,并把原因带上 ——
/// 断网时看见「3 台设备,云端暂时联系不上」远好过看见一片空白。
#[cfg_attr(feature = "desktop", tauri::command)]
pub async fn mesh_devices() -> Result<Value, String> {
    let me = my_node_id();
    let (cloud, cloud_msg) = match cfg() {
        Some(c) => {
            let r = tokio::task::spawn_blocking(move || -> Result<Value, String> {
                let resp = agent()?
                    .get(&format!("{}/api/mesh/devices", c.url))
                    .set("Authorization", &format!("Bearer {}", c.key))
                    .call()
                    .map_err(cloud_err)?;
                resp.into_json::<Value>()
                    .map_err(|e| format!("云端返回的不是 JSON:{e}"))
            })
            .await
            .unwrap_or_else(|e| Err(format!("取台账任务失败:{e}")));
            match r {
                Ok(v) => (v, String::new()),
                Err(e) => (json!({}), e),
            }
        }
        None => (json!({}), "本机还没登录".to_string()),
    };

    // 本机契约:node_id → 档位。
    let grants: HashMap<String, crate::collab::grants::Grant> = crate::collab::grants::list()
        .unwrap_or_default()
        .into_iter()
        .map(|g| (g.node_id.clone(), g))
        .collect();
    // 实时链路 + 盘。
    let mounts: HashMap<String, Value> = crate::fsmount::fs_mount_status()
        .into_iter()
        .filter_map(|v| {
            v.get("sourceId")
                .and_then(|x| x.as_str())
                .map(|s| (s.to_string(), v.clone()))
        })
        .collect();
    let links_snapshot: HashMap<String, (bool, String, String, String, u64)> = links()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .iter()
        .map(|(k, l)| {
            let connected = link_connected(l);
            (
                k.clone(),
                (
                    connected,
                    l.err.clone(),
                    l.phase.clone(),
                    l.error_code.clone(),
                    l.last_verified_at,
                ),
            )
        })
        .collect();

    // 云端台账为骨架;云端拉不到就拿本机契约当骨架(断网也要能看见自己批过哪些设备)。
    let skeleton: Vec<Value> = match cloud.get("devices").and_then(|x| x.as_array()) {
        Some(list) => list.clone(),
        None => grants
            .values()
            .map(|g| {
                json!({
                    "nodeId": g.node_id, "name": g.name, "os": "", "ver": "",
                    "lastSeen": 0, "online": false, "firstSeen": g.granted_at,
                    "revoked": g.revoked, "self": g.node_id == me,
                })
            })
            .collect(),
    };

    let devices: Vec<Value> = skeleton
        .into_iter()
        .map(|mut d| {
            let nid = d
                .get("nodeId")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let g = grants.get(&nid);
            let m = mounts.get(&source_id(&nid));
            let (connected, err, phase, error_code, last_verified_at) = links_snapshot
                .get(&nid)
                .cloned()
                .unwrap_or((false, String::new(), "stopped".into(), String::new(), 0));
            d["isSelf"] = json!(d.get("self").and_then(|x| x.as_bool()).unwrap_or(nid == me));
            d["connected"] = json!(connected);
            d["phase"] = json!(phase);
            d["errorCode"] = json!(error_code);
            d["lastVerifiedAt"] = json!(last_verified_at);
            d["error"] = json!(err);
            d["drive"] = m.and_then(|x| x.get("drive")).cloned().unwrap_or(json!(""));
            d["mounted"] = m.and_then(|x| x.get("ok")).cloned().unwrap_or(json!(false));
            d["writable"] = m
                .and_then(|x| x.get("writable"))
                .cloned()
                .unwrap_or(json!(false));
            // 档位:没有契约的设备(还没连过)给个「未批准」而不是编一个默认值。
            d["fsAccess"] = json!(g.map(|x| x.fs_access.clone()).unwrap_or_default());
            d["effectiveFs"] = json!(g.map(|x| x.effective_fs().to_string()).unwrap_or_default());
            d["execAccess"] = json!(g.map(|x| x.exec_access.clone()).unwrap_or_default());
            d["autoMount"] = json!(g.map(|x| x.auto_mount).unwrap_or(false));
            d["inCooldown"] = json!(false);
            d["cooldownUntil"] = json!(0);
            d["granted"] = json!(g.is_some());
            d["localRevoked"] = json!(g.map(|x| x.revoked).unwrap_or(false));
            d
        })
        .collect();

    let uid = visible_uid();
    let (enrollment_state, ready, secure, enrollment_warn) = enrollment_state();
    Ok(json!({
        "enrolled": cfg().is_some(),
        "ready": ready,
        "secure": secure,
        "enrollmentState": enrollment_state,
        "enrollmentWarn": enrollment_warn,
        "interconnect": orchestration::status(),
        "authorityConfigured": !default_authority().is_empty(),
        "configurationError": crate::collab::interconnect::load().err(),
        "relayConfig": orchestration::relay_status(),
        "url": meta(K_URL),
        "uid": uid,
        "nodeId": me,
        "name": my_name(),
        "cloudError": cloud_msg,
        // 「这台机器认不认我」。false 时别的设备一律连不进来(它们进的就是这道门),
        // UI 据此把话说明白 + 给出邀请码那条出路,而不是让用户对着一片"看得见连不上"猜。
        "localMember": crate::collab::authority::is_member(&uid),
        "localMemberWarn": member_warn().lock().unwrap_or_else(|error| error.into_inner()).clone(),
        "devices": devices,
    }))
}


/// 立刻对一次账(UI 点「刷新」用,不必等下一拍心跳)。
#[cfg_attr(feature = "desktop", tauri::command)]
pub async fn mesh_sync() -> Result<Value, String> {
    let n = reconcile_once().await?;
    Ok(json!({ "peers": n }))
}

// ────────────────────────── 设备码:一码互联 ──────────────────────────
//
// 「主机」这个概念到此为止。每台机器都有一串自己的码,都能被连、也都能去连别人 ——
// 界面上不再有「设为主机」「owner 令牌」「邀请码」「入伙」这些只有我们自己懂的词。
//
// 码的形态:`PLR1-<访问码>-<NodeId>`。
// 为什么 NodeId 得在码里而不是像 UU远程 那样只给一串短数字:UU 有一台云端目录服务器
// 替它把「ID → 现在在哪」查出来,我们的云端账号中心只认**同一个账号**的设备,
// 别人的账号在上面查不到你。要做成 12 位短码,得在云机上再加一张公开的
// 「码 → NodeId」目录表并重新部署它 —— 那是另一步,得你点头。现在这串码的好处是
// **不依赖云端**:两台机器都断了公网、只在一个局域网里,照样粘码就连上。

/// 本机设备码。用户复制的就是这一串。
#[cfg_attr(feature = "desktop", tauri::command)]
pub async fn device_code() -> Result<Value, String> {
    let node_id = my_node_id();
    let code = tokio::task::spawn_blocking(crate::collab::identity::access_code)
        .await
        .map_err(|e| format!("取设备码失败:{e}"))??;
    Ok(json!({
        "code": code,
        "nodeId": node_id,
        "name": my_name(),
        // NodeId 还没就绪(隧道没起来)时给半成品:UI 照实说「稍候」,别显示一串连不上的码。
        "share": if node_id.is_empty() { String::new() } else { format!("PLR1-{code}-{node_id}") },
        "ready": !node_id.is_empty(),
    }))
}

/// 换一串码:旧码当场失效,靠旧码连进来的设备会话一并作废。
#[cfg_attr(feature = "desktop", tauri::command)]
pub async fn device_code_rotate() -> Result<Value, String> {
    let code = tokio::task::spawn_blocking(|| crate::collab::identity::rotate_access_code("本机"))
        .await
        .map_err(|e| format!("换码任务失败:{e}"))??;
    let node_id = my_node_id();
    Ok(json!({
        "code": code,
        "share": if node_id.is_empty() { String::new() } else { format!("PLR1-{code}-{node_id}") },
    }))
}

/// 解析一串设备码 → (访问码, NodeId)。只认 `PLR1-<code>-<nodeId>`。
pub(super) fn parse_device_code(s: &str) -> Result<(String, String), String> {
    let t = s.trim();
    if t.to_ascii_uppercase().starts_with("PLRK1-") {
        return Err("legacy_code_unsupported:这是旧版 PLRK1 设备码，缺少一次性挑战和主机密钥证明，不能安全降级使用；请让对方升级 Polaris 后重新复制 PLR1 设备码".into());
    }
    let body = t
        .strip_prefix("PLR1-")
        .or_else(|| t.strip_prefix("plr1-"))
        .ok_or("这不是设备码 —— 它长这样:PLR1-XXXXXXXX-<一串字母数字>")?;
    let (code, node) = body
        .split_once('-')
        .ok_or("设备码不完整,少了后半段 —— 让对方整串重新复制一次")?;
    if code.trim().is_empty() || node.trim().is_empty() {
        return Err("设备码不完整 —— 让对方整串重新复制一次".into());
    }
    Ok((code.trim().to_ascii_uppercase(), node.trim().to_string()))
}

/// 设备网现状:入网了吗、名册上有谁、各自连上了没、挂成了哪块盘。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn mesh_status() -> Value {
    let enrolled = cfg().is_some();
    let (enrollment_state, ready, secure, enrollment_warn) = enrollment_state();
    // fsmount 的实况(盘符/读写档/在线)按 sourceId 对进来,前端一处就能全画出来。
    let mounts: HashMap<String, Value> = crate::fsmount::fs_mount_status()
        .into_iter()
        .filter_map(|v| {
            v.get("sourceId")
                .and_then(|x| x.as_str())
                .map(|s| (s.to_string(), v.clone()))
        })
        .collect();
    let peers: Vec<Value> = links()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .iter()
        .map(|(node_id, l)| {
            let m = mounts.get(&source_id(node_id));
            json!({
                "nodeId": node_id,
                "name": l.name,
                "port": l.port,
                "connected": link_connected(l),
                "phase": l.phase,
                "errorCode": l.error_code,
                "lastVerifiedAt": l.last_verified_at,
                "error": l.err,
                // 对端会话 token 交给前端:远程终端(/api/exec)与「浏览盘」都靠它。
                // 与手工添加的远程源同一口径 —— 那条路的 owner 令牌本来就存在前端。
                "token": l.token.clone().unwrap_or_default(),
                "drive": m.and_then(|x| x.get("drive")).cloned().unwrap_or(json!("")),
                "writable": m.and_then(|x| x.get("writable")).cloned().unwrap_or(json!(false)),
                "ok": m.and_then(|x| x.get("ok")).cloned().unwrap_or(json!(false)),
            })
        })
        .collect();
    json!({
        "enrolled": enrolled,
        "ready": ready,
        "secure": secure,
        "enrollmentState": enrollment_state,
        "enrollmentWarn": enrollment_warn,
        "interconnect": orchestration::status(),
        "authorityConfigured": !default_authority().is_empty(),
        "configurationError": crate::collab::interconnect::load().err(),
        "relayConfig": orchestration::relay_status(),
        "url": meta(K_URL),
        "uid": visible_uid(),
        "nodeId": my_node_id(),
        "name": my_name(),
        "peers": peers,
    })
}


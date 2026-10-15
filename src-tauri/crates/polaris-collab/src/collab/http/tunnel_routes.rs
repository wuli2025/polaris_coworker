use super::*;

// ── iroh 隧道控制 ──

pub(super) async fn tunnel_status_api(State(state): State<CollabState>, headers: HeaderMap) -> Response {
    let Some(_ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    #[cfg(feature = "collab-net")]
    {
        return Json(crate::collab::tunnel::status()).into_response();
    }
    #[cfg(not(feature = "collab-net"))]
    Json(json!({"running": false, "unavailable": "本构建未启用 collab-net(iroh) 功能"}))
        .into_response()
}

pub(super) async fn tunnel_start_api(State(state): State<CollabState>, headers: HeaderMap) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    if role_rank(&ctx.role) < 3 {
        return forbid();
    }
    #[cfg(feature = "collab-net")]
    {
        crate::collab::tunnel::start_host_blocking_thread();
        return Json(json!({"ok": true})).into_response();
    }
    #[cfg(not(feature = "collab-net"))]
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(json!({"error":"本构建未启用 collab-net 功能"})),
    )
        .into_response()
}

/// RelayMap 动态下发(v8 7.3 第2项):改中继只动主机一处,全员自动生效。
pub(super) async fn tunnel_relays_api(
    State(state): State<CollabState>,
    headers: HeaderMap,
    Json(_v): Json<Value>,
) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    if role_rank(&ctx.role) < 3 {
        return forbid();
    }
    #[cfg(feature = "collab-net")]
    {
        return match crate::collab::tunnel::apply_relay_config(&_v.to_string()) {
            Ok(_) => Json(json!({"ok": true})).into_response(),
            Err(e) => (StatusCode::BAD_REQUEST, Json(json!({"error": e}))).into_response(),
        };
    }
    #[cfg(not(feature = "collab-net"))]
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(json!({"error":"本构建未启用 collab-net 功能"})),
    )
        .into_response()
}

// ── 云机中继网关:桌面主机挂牌注册 ──

/// 桌面主机挂牌到云机网关:POST 自己的 iroh NodeId,云机起本地桥接监听并返回网关 NodeId
/// (桌面随后把它加进设备白名单,隧道才放行)。
/// 鉴权:**必须真实账号**(user_id>0)。全局口令/免鉴合成身份一律拒 —— 挂牌要绑到账号上,
/// 账号管理面才能按账号看到「谁挂了哪台主机」并一键踢下线。NodeId 经 ensure_device_for
/// 绑定该账号(已属别人的 NodeId 拒绝顶替),挂牌落库 gw_hosts(云机重启懒恢复)。
#[cfg(feature = "collab-net")]
pub(super) async fn gw_register(
    State(state): State<CollabState>,
    headers: HeaderMap,
    Json(v): Json<Value>,
) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    if ctx.user_id <= 0 {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error":"挂牌必须使用真实账号登录(免密口令身份不可注册主机)"})),
        )
            .into_response();
    }
    let host_node_id = s_of(&v, "hostNodeId");
    if host_node_id.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"缺少 hostNodeId"})),
        )
            .into_response();
    }
    let host_name = s_of(&v, "hostName");
    let name = if host_name.trim().is_empty() {
        "桌面主机".to_string()
    } else {
        host_name
    };
    // NodeId ↔ 账号绑定(防顶替):已属其他账号直接拒。
    {
        let uid = ctx.user_id;
        let node = host_node_id.clone();
        let nm = name.clone();
        let bound = tokio::task::spawn_blocking(move || {
            crate::collab::identity::ensure_device_for(uid, &node, &nm)
        })
        .await;
        match bound {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => {
                return (StatusCode::FORBIDDEN, Json(json!({"error": e}))).into_response()
            }
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": format!("设备绑定任务失败: {e}")})),
                )
                    .into_response()
            }
        }
    }
    crate::collab::db::audit(&ctx.username, "gw.register", &host_node_id, &name);
    match crate::collab::gateway::register_host(&host_node_id, &name, ctx.user_id).await {
        Ok(port) => Json(json!({
            "ok": true,
            "gatewayNodeId": crate::collab::gateway::gateway_node_id().unwrap_or_default(),
            "hostId": host_node_id,
            "localPort": port,
        }))
        .into_response(),
        Err(e) => (StatusCode::BAD_GATEWAY, Json(json!({"error": e}))).into_response(),
    }
}

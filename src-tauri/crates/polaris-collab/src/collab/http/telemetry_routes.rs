use super::*;

// ── 设备遥测(设备联盟 Phase 2):成员设备定时上报资源实况,内存态聚合 ──
// 键 = 上报方 deviceId(登录时登记为 devices.node_id)。只留最新一帧,不落库;
// 设备看板(collab_devices)按 node_id 合并出 stats/stats_at,前端如实渲染 —— 不插值不造假。
static TELEMETRY: Lazy<parking_lot::Mutex<HashMap<String, (Value, i64)>>> =
    Lazy::new(|| parking_lot::Mutex::new(HashMap::new()));
const TELEMETRY_MAX_DEVICES: usize = 256;
const TELEMETRY_MAX_BYTES: usize = 2048;

pub(super) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub(super) async fn collab_telemetry_report(
    State(state): State<CollabState>,
    headers: HeaderMap,
    Json(v): Json<Value>,
) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    if role_rank(&ctx.role) < 1 {
        return forbid();
    }
    let dev = s_of(&v, "deviceId");
    if dev.is_empty() || dev.len() > 128 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"deviceId 无效"})),
        )
            .into_response();
    }
    let stats = v.get("stats").cloned().unwrap_or(Value::Null);
    if !stats.is_object() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"stats 须为对象"})),
        )
            .into_response();
    }
    if serde_json::to_string(&stats)
        .map(|s| s.len())
        .unwrap_or(usize::MAX)
        > TELEMETRY_MAX_BYTES
    {
        return (StatusCode::BAD_REQUEST, Json(json!({"error":"stats 过大"}))).into_response();
    }
    // 设备身份绑定:node_id(= deviceId)必须属当前账号(空闲则登记为其设备)。
    // 挡住成员用别台 node_id 顶替上报假数据;owner 手机首次上报即补一台设备行,遥测有卡承载。
    let name = {
        let n = s_of(&v, "name");
        if n.is_empty() {
            format!("{} 的设备", ctx.username)
        } else {
            n.chars().take(48).collect()
        }
    };
    let uid = ctx.user_id;
    let dev_bind = dev.clone();
    let bound = tokio::task::spawn_blocking(move || {
        crate::collab::identity::ensure_device_for(uid, &dev_bind, &name)
    })
    .await;
    match bound {
        Ok(Ok((_, is_new))) => {
            if is_new {
                // 首次登记留痕(只此一次,不随每帧刷 —— 避免 audit 写放大)。
                let actor = ctx.username.clone();
                let d2 = dev.clone();
                tokio::task::spawn_blocking(move || {
                    crate::collab::db::audit(&actor, "device.telemetry", &d2, "设备开始上报资源")
                });
            }
        }
        Ok(Err(e)) => return (StatusCode::FORBIDDEN, Json(json!({"error": e}))).into_response(),
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error":"内部错误"})),
            )
                .into_response()
        }
    }
    {
        let mut map = TELEMETRY.lock();
        // 容量闸:满了先淘汰最旧一帧,防失控设备撑爆内存。
        if !map.contains_key(&dev) && map.len() >= TELEMETRY_MAX_DEVICES {
            if let Some(oldest) = map
                .iter()
                .min_by_key(|(_, (_, at))| *at)
                .map(|(k, _)| k.clone())
            {
                map.remove(&oldest);
            }
        }
        map.insert(dev, (stats, now_ms()));
    }
    Json(json!({"ok": true})).into_response()
}

pub(super) async fn collab_devices(State(state): State<CollabState>, headers: HeaderMap) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    if role_rank(&ctx.role) < 3 {
        return forbid();
    }
    let out = tokio::task::spawn_blocking(|| -> Result<Value, String> {
        // 附 is_host:node_id 命中 meta.host_node_id 的那台就是主机(设备页徽标)。
        let host_node = crate::collab::db::meta_get("host_node_id").unwrap_or_default();
        let list = crate::collab::identity::list_devices()?;
        // 遥测合并:node_id(= 登录时的 deviceId)命中上报表 → 附最新一帧 stats。
        let telem = TELEMETRY.lock().clone();
        let out: Vec<Value> = list
            .into_iter()
            .map(|d| {
                let is_host = !host_node.is_empty() && d.node_id == host_node;
                let mut j = serde_json::to_value(&d).unwrap_or_else(|_| json!({}));
                if is_host {
                    j["is_host"] = json!(true);
                }
                if let Some((stats, at)) = telem.get(&d.node_id) {
                    j["stats"] = stats.clone();
                    j["stats_at"] = json!(at);
                }
                j
            })
            .collect();
        ok(out)
    })
    .await;
    unwrap_api(out)
}

/// 最近活动(audit 表,owner):设备接入/上报/吊销、账号事件 —— 「正在发生」tab 数据源。
pub(super) async fn collab_audit_recent(
    State(state): State<CollabState>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    if role_rank(&ctx.role) < 3 {
        return forbid();
    }
    let limit = q
        .get("limit")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(50)
        .clamp(1, 200);
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        ok(crate::collab::db::audit_recent(limit)?)
    })
    .await;
    unwrap_api(out)
}

pub(super) async fn collab_device_revoke(
    State(state): State<CollabState>,
    headers: HeaderMap,
    Json(v): Json<Value>,
) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    if role_rank(&ctx.role) < 3 {
        return forbid();
    }
    let out = tokio::task::spawn_blocking(move || {
        let node_id = crate::collab::identity::revoke_device(&s_of(&v, "deviceId"))?;
        // 吊销即把这台设备在中继上的挂牌踢下线(若有);反代闸随后也会拒它。
        #[cfg(feature = "collab-net")]
        crate::collab::gateway::unregister_node(&node_id);
        #[cfg(not(feature = "collab-net"))]
        let _ = node_id;
        Ok(json!({"ok": true}))
    })
    .await;
    unwrap_api(out)
}

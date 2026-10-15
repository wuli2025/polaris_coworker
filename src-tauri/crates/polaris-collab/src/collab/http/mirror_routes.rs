use super::*;

// ── 账号镜像(云端双存) ──

/// 主机侧:导出加密镜像(owner)。返回 blob,调用方可 POST 给云端 /mirror/store。
pub(super) async fn mirror_export_api(State(state): State<CollabState>, headers: HeaderMap) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    if role_rank(&ctx.role) < 3 {
        return forbid();
    }
    let out = tokio::task::spawn_blocking(|| {
        crate::collab::account_store::export_blob().map(|b| json!({"blob": b}))
    })
    .await;
    unwrap_api(out)
}

/// 云端侧:保管密文(owner 会话或全局口令)。云端解不开,只做保管。
pub(super) async fn mirror_store_api(
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
        crate::collab::account_store::store_remote_blob(&s_of(&v, "blob"))
            .map(|ver| json!({"ok": true, "version": ver}))
    })
    .await;
    unwrap_api(out)
}

pub(super) async fn mirror_pull_api(State(state): State<CollabState>, headers: HeaderMap) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    if role_rank(&ctx.role) < 3 {
        return forbid();
    }
    let out = tokio::task::spawn_blocking(|| {
        crate::collab::account_store::load_blob().map(|o| match o {
            Some((ver, blob)) => json!({"version": ver, "blob": blob}),
            None => json!({"version": 0, "blob": null}),
        })
    })
    .await;
    unwrap_api(out)
}

/// 恢复(空库 + 持有 host.key 的机器上才可能成功;account_store 内部还有防覆盖闸)。
pub(super) async fn mirror_restore_api(
    State(state): State<CollabState>,
    headers: HeaderMap,
    Json(v): Json<Value>,
) -> Response {
    // 灾后恢复也必须持机器访问口令；空库不再自动合成 owner。
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    if role_rank(&ctx.role) < 3 {
        return forbid();
    }
    let out = tokio::task::spawn_blocking(move || {
        crate::collab::account_store::restore_blob(&s_of(&v, "blob"))
            .map(|n| json!({"ok": true, "restored": n}))
    })
    .await;
    unwrap_api(out)
}

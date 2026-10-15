use super::*;

/// 用户名搜索(拉人自动补全)。登录即可用,只回不敏感字段。
pub(super) async fn collab_user_search(
    State(state): State<CollabState>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let Some(_ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    let kw = q.get("q").cloned().unwrap_or_default();
    let out =
        tokio::task::spawn_blocking(move || crate::collab::teams::search_users(&kw).and_then(ok))
            .await;
    unwrap_api(out)
}

pub(super) async fn team_list(State(state): State<CollabState>, headers: HeaderMap) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    let out = tokio::task::spawn_blocking(move || {
        crate::collab::teams::list_mine(ctx.user_id).and_then(ok)
    })
    .await;
    unwrap_api(out)
}

/// 建团队:任何登录用户(访问者除外)。
pub(super) async fn team_create(
    State(state): State<CollabState>,
    headers: HeaderMap,
    Json(v): Json<Value>,
) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    if role_rank(&ctx.role) < 2 {
        return forbid();
    }
    let out = tokio::task::spawn_blocking(move || {
        crate::collab::teams::create(&s_of(&v, "name"), ctx.user_id, &ctx.username).and_then(ok)
    })
    .await;
    unwrap_api(out)
}

/// 团队管理权:全局 owner 或该团队 owner。
pub(super) fn is_team_admin(ctx: &AuthCtx, team_id: i64) -> bool {
    role_rank(&ctx.role) >= 3
        || crate::collab::teams::my_role(team_id, ctx.user_id).as_deref() == Some("owner")
}

pub(super) async fn team_members_api(
    State(state): State<CollabState>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    let Some(tid) = q.get("teamId").and_then(|s| s.parse::<i64>().ok()) else {
        return (StatusCode::BAD_REQUEST, Json(json!({"error":"缺 teamId"}))).into_response();
    };
    // 团队成员可看成员列表
    if role_rank(&ctx.role) < 3 && crate::collab::teams::my_role(tid, ctx.user_id).is_none() {
        return (StatusCode::FORBIDDEN, Json(json!({"error":"你不在该团队"}))).into_response();
    }
    let out =
        tokio::task::spawn_blocking(move || crate::collab::teams::members(tid).and_then(ok)).await;
    unwrap_api(out)
}

/// 按用户名拉人(GitHub 式邀请)。团队 owner 专属。
pub(super) async fn team_member_add(
    State(state): State<CollabState>,
    headers: HeaderMap,
    Json(v): Json<Value>,
) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    let Some(tid) = i_of(&v, "teamId") else {
        return (StatusCode::BAD_REQUEST, Json(json!({"error":"缺 teamId"}))).into_response();
    };
    if !is_team_admin(&ctx, tid) {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error":"只有团队管理者能拉人"})),
        )
            .into_response();
    }
    let out = tokio::task::spawn_blocking(move || {
        let name = crate::collab::teams::add_member_by_username(
            tid,
            &s_of(&v, "username"),
            &s_of(&v, "role"),
            &ctx.username,
        )?;
        Ok(json!({"ok": true, "username": name}))
    })
    .await;
    unwrap_api(out)
}

pub(super) async fn team_member_remove(
    State(state): State<CollabState>,
    headers: HeaderMap,
    Json(v): Json<Value>,
) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    let (Some(tid), Some(uid)) = (i_of(&v, "teamId"), i_of(&v, "userId")) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"缺 teamId/userId"})),
        )
            .into_response();
    };
    // 自己可以退出;移除别人要团队管理权
    if uid != ctx.user_id && !is_team_admin(&ctx, tid) {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error":"只有团队管理者能移除成员"})),
        )
            .into_response();
    }
    let out = tokio::task::spawn_blocking(move || {
        crate::collab::teams::remove_member(tid, uid, &ctx.username).map(|_| json!({"ok": true}))
    })
    .await;
    unwrap_api(out)
}

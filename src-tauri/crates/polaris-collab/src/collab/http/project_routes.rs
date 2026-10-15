use super::*;

// ── 项目与任务卡 ──
//
// 权限模型(确定性两问):① 全局角色够不够(role_rank);② 是不是项目成员
// (project_members 表;全局 owner 与合成 owner 免查——他们是主机管理员)。

/// 项目成员校验:owner 直通;其余必须在成员表。
pub(super) fn ensure_member(ctx: &AuthCtx, project_id: i64) -> Result<(), Response> {
    if role_rank(&ctx.role) >= 3 {
        return Ok(());
    }
    if crate::collab::projects::member_role(project_id, ctx.user_id).is_some() {
        return Ok(());
    }
    Err((
        StatusCode::FORBIDDEN,
        Json(json!({"error":"你不是该项目成员"})),
    )
        .into_response())
}

/// 「纯账号中心」模式:本进程**只**当身份权威,不当协作主机。
///
/// 为什么需要它:账号中心的 `users` 表同时就是这台机器自己的成员表 —— 开放自助注册,
/// 等于任何人都能拿到本机的 collaborator 会话,而成员能建项目、触发 checks 跑构建脚本,
/// 本质是主机 RCE。「填个邮箱就能用」要求官方云机必须开放注册,于是这道闸必须同时上:
/// 把云机上「能跑东西」的那一面整个关掉,成员再多也没东西可碰。
///
/// 桌面/NAS 主机不设这个变量,一切照旧。
pub fn authority_only() -> bool {
    std::env::var("POLARIS_AUTHORITY_ONLY")
        .map(|s| s == "1" || s.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// 纯账号中心上被关掉的能力,统一这一句话。
pub(super) fn authority_only_denied() -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(json!({
            "error":"这台服务器只做账号中心,不承载项目与任务 —— 请在你自己的电脑/NAS 上操作"
        })),
    )
        .into_response()
}

/// 建项目会授予后续 git/check/merge 能力，因此只允许主机管理员绑定本地仓库。
pub(super) async fn project_create(
    State(state): State<CollabState>,
    headers: HeaderMap,
    Json(v): Json<Value>,
) -> Response {
    if authority_only() {
        return authority_only_denied();
    }
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    let team_id = i_of(&v, "teamId");
    if role_rank(&ctx.role) < 3 {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error":"只有主机管理员能创建项目并绑定本地仓库"})),
        )
            .into_response();
    }
    let out = tokio::task::spawn_blocking(move || {
        let repo = validate_repo_path(&s_of(&v, "repo"))?;
        crate::collab::projects::create(
            &s_of(&v, "name"),
            &repo.to_string_lossy(),
            team_id,
            ctx.user_id,
            &ctx.username,
        )
        .and_then(ok)
    })
    .await;
    unwrap_api(out)
}

pub(super) async fn project_list(State(state): State<CollabState>, headers: HeaderMap) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    let is_owner = role_rank(&ctx.role) >= 3;
    let out = tokio::task::spawn_blocking(move || {
        crate::collab::projects::list_for(ctx.user_id, is_owner).and_then(ok)
    })
    .await;
    unwrap_api(out)
}

pub(super) async fn project_members(
    State(state): State<CollabState>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    let Some(pid) = q.get("projectId").and_then(|s| s.parse::<i64>().ok()) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"缺 projectId"})),
        )
            .into_response();
    };
    if let Err(r) = ensure_member(&ctx, pid) {
        return r;
    }
    let out =
        tokio::task::spawn_blocking(move || crate::collab::projects::members(pid).and_then(ok))
            .await;
    unwrap_api(out)
}

/// 加项目成员:项目/团队管理者即可;支持 userId 或 username(GitHub 式搜索拉人)。
pub(super) async fn project_member_add(
    State(state): State<CollabState>,
    headers: HeaderMap,
    Json(v): Json<Value>,
) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    let Some(pid) = i_of(&v, "projectId") else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"缺 projectId"})),
        )
            .into_response();
    };
    let global_owner = role_rank(&ctx.role) >= 3;
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        if !crate::collab::projects::can_admin(pid, ctx.user_id, global_owner) {
            return Err("只有项目/团队管理者能加成员".into());
        }
        let role = {
            let r = s_of(&v, "role");
            if r.is_empty() {
                "collaborator".into()
            } else {
                r
            }
        };
        if let Some(uid) = i_of(&v, "userId") {
            crate::collab::projects::add_member(pid, uid, &role, &ctx.username)?;
        } else {
            let uname = s_of(&v, "username");
            if uname.is_empty() {
                return Err("缺 userId 或 username".into());
            }
            crate::collab::projects::add_member_by_username(pid, &uname, &role, &ctx.username)?;
        }
        Ok(json!({"ok": true}))
    })
    .await;
    unwrap_api(out)
}

pub(super) async fn project_set_lead(
    State(state): State<CollabState>,
    headers: HeaderMap,
    Json(v): Json<Value>,
) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    let global_owner = role_rank(&ctx.role) >= 3;
    let out = tokio::task::spawn_blocking(move || {
        let pid = i_of(&v, "projectId").ok_or("缺 projectId")?;
        if !crate::collab::projects::can_admin(pid, ctx.user_id, global_owner) {
            return Err("只有项目/团队管理者能任命主 Agent".into());
        }
        let expert = s_of(&v, "expertId");
        let expert = if expert.is_empty() {
            None
        } else {
            Some(expert)
        };
        crate::collab::projects::set_lead(pid, expert.as_deref(), &ctx.username)
            .map(|_| json!({"ok": true}))
    })
    .await;
    unwrap_api(out)
}

/// 设项目共享可见路径(CSV):协作者开工时并入稀疏集(scope ∪ shared_scope)。管理者专属。
pub(super) async fn project_shared_scope_set(
    State(state): State<CollabState>,
    headers: HeaderMap,
    Json(v): Json<Value>,
) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    let global_owner = role_rank(&ctx.role) >= 3;
    let out = tokio::task::spawn_blocking(move || {
        let pid = i_of(&v, "projectId").ok_or("缺 projectId")?;
        if !crate::collab::projects::can_admin(pid, ctx.user_id, global_owner) {
            return Err("只有项目/团队管理者能改共享可见路径".into());
        }
        crate::collab::projects::set_shared_scope(pid, &s_of(&v, "sharedScope"), &ctx.username)
            .map(|_| json!({"ok": true}))
    })
    .await;
    unwrap_api(out)
}

pub(super) async fn task_list(
    State(state): State<CollabState>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    let Some(pid) = q.get("projectId").and_then(|s| s.parse::<i64>().ok()) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"缺 projectId"})),
        )
            .into_response();
    };
    if let Err(r) = ensure_member(&ctx, pid) {
        return r;
    }
    let out =
        tokio::task::spawn_blocking(move || crate::collab::tasks::list(pid).and_then(ok)).await;
    unwrap_api(out)
}

/// 任务卡变更统一广播(看板实时刷新)。项目级事件,广播即可;个人定向流走 emit_to。
pub(super) fn emit_task(state: &CollabState, card: &crate::collab::tasks::TaskCard) {
    let _ = state.app.emit("collab:task", card);
}

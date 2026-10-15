use super::*;

// ── 任务级对话(多轮微调通道,详见 chat.rs)──

/// 消息鉴权 + 身份归类:项目成员可读写;返回 (card, 消息 role)。
pub(super) fn chat_access(
    ctx: &AuthCtx,
    task_id: i64,
) -> Result<(crate::collab::tasks::TaskCard, String), Response> {
    let card = crate::collab::tasks::get(task_id)
        .map_err(|e| (StatusCode::NOT_FOUND, Json(json!({"error": e}))).into_response())?;
    ensure_member(ctx, card.project_id)?;
    let global_owner = role_rank(&ctx.role) >= 3;
    let role = if card.assignee == Some(ctx.user_id) {
        "assignee"
    } else if crate::collab::projects::can_admin(card.project_id, ctx.user_id, global_owner) {
        "lead"
    } else {
        "member"
    };
    Ok((card, role.to_string()))
}

pub(super) async fn task_messages_get(
    State(state): State<CollabState>,
    headers: HeaderMap,
    axum::extract::Path(tid): axum::extract::Path<i64>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    let after_id = q
        .get("afterId")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0);
    let limit = q
        .get("limit")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(100);
    let out = tokio::task::spawn_blocking(move || -> Result<Value, Response> {
        chat_access(&ctx, tid)?;
        crate::collab::chat::list(tid, after_id, limit)
            .and_then(ok)
            .map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({"error": e}))).into_response())
    })
    .await;
    match out {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(resp)) => resp,
        Err(e) => err_resp(format!("内部任务失败: {e}")),
    }
}

pub(super) async fn task_messages_post(
    State(state): State<CollabState>,
    headers: HeaderMap,
    axum::extract::Path(tid): axum::extract::Path<i64>,
    Json(v): Json<Value>,
) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    if role_rank(&ctx.role) < 2 {
        return forbid();
    }
    let st = state.clone();
    let out = tokio::task::spawn_blocking(move || -> Result<Value, Response> {
        let (_card, role) = chat_access(&ctx, tid)?;
        let idem = v.get("idemKey").and_then(|k| k.as_str());
        let msg = crate::collab::chat::post(
            tid,
            ctx.user_id,
            &ctx.username,
            &role,
            &s_of(&v, "body"),
            idem,
        )
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({"error": e}))).into_response())?;
        // 项目级广播:任务抽屉聊天面板实时追加(/ws 与看板事件同通道)。
        let _ = st.app.emit("collab:task_message", &msg);
        ok(msg).map_err(|e| err_resp(e))
    })
    .await;
    match out {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(resp)) => resp,
        Err(e) => err_resp(format!("内部任务失败: {e}")),
    }
}

/// 主 Agent 在任务对话里回一条(lead/管理者手动触发,控制 token 用量)。
/// AI 输出仍只是建议:写进消息流,不碰状态机。
pub(super) async fn task_ai_reply(
    State(state): State<CollabState>,
    headers: HeaderMap,
    axum::extract::Path(tid): axum::extract::Path<i64>,
) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    let st = state.clone();
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let card = crate::collab::tasks::get(tid)?;
        let global_owner = role_rank(&ctx.role) >= 3;
        if !crate::collab::projects::can_admin(card.project_id, ctx.user_id, global_owner) {
            return Err("只有项目/团队管理者能触发主 Agent 回复".into());
        }
        let _run_guard = LeadAiRunGuard::acquire(card.project_id)?;
        let text = crate::collab::lead_ai::ai_task_reply(card.project_id, tid)?;
        let msg = crate::collab::chat::post(tid, 0, "主Agent", "ai", &text, None)?;
        let _ = st.app.emit("collab:task_message", &msg);
        ok(msg)
    })
    .await;
    unwrap_api(out)
}

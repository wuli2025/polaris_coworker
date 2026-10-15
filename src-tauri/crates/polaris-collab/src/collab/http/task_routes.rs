use super::*;

pub(super) async fn task_create(
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
    let Some(pid) = i_of(&v, "projectId") else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"缺 projectId"})),
        )
            .into_response();
    };
    if role_rank(&ctx.role) < 2 {
        return forbid();
    }
    if let Err(r) = ensure_member(&ctx, pid) {
        return r;
    }
    let st = state.clone();
    let out = tokio::task::spawn_blocking(move || {
        let card = crate::collab::tasks::create(
            pid,
            &s_of(&v, "title"),
            &s_of(&v, "body"),
            &s_of(&v, "scope"),
            &s_of(&v, "criteria"),
            &ctx.username,
        )?;
        emit_task(&st, &card);
        ok(card)
    })
    .await;
    unwrap_api(out)
}

/// 卡操作通用骨架:取卡→项目成员校验→执行→广播。
macro_rules! task_op {
    ($fn_name:ident, $body:expr) => {
        pub(super) async fn $fn_name(State(state): State<CollabState>, headers: HeaderMap, Json(v): Json<Value>) -> Response {
            let Some(ctx) = auth_ctx(&state, &headers).await else { return forbid(); };
            if role_rank(&ctx.role) < 2 { return forbid(); }
            let Some(tid) = i_of(&v, "taskId") else {
                return (StatusCode::BAD_REQUEST, Json(json!({"error":"缺 taskId"}))).into_response();
            };
            let pid = match tokio::task::spawn_blocking(move || crate::collab::tasks::get(tid).map(|c| c.project_id)).await {
                Ok(Ok(p)) => p,
                Ok(Err(e)) => return (StatusCode::BAD_REQUEST, Json(json!({"error": e}))).into_response(),
                Err(e) => return err_resp(e.to_string()),
            };
            if let Err(r) = ensure_member(&ctx, pid) { return r; }
            let st = state.clone();
            let op: fn(i64, &AuthCtx, &Value, &CollabState) -> Result<Value, String> = $body;
            let out = tokio::task::spawn_blocking(move || op(tid, &ctx, &v, &st)).await;
            unwrap_api(out)
        }
    };
}

task_op!(task_claim, |tid, ctx, _v, st| {
    let card = crate::collab::tasks::claim(tid, ctx.user_id, &ctx.username)?;
    emit_task(st, &card);
    ok(card)
});

task_op!(task_submit, |tid, ctx, v, st| {
    let before = crate::collab::tasks::get(tid)?;
    let can_admin = crate::collab::projects::can_admin(
        before.project_id,
        ctx.user_id,
        role_rank(&ctx.role) >= 3,
    );
    if before.assignee != Some(ctx.user_id) && !can_admin {
        return Err("只有任务负责人或项目管理者能提交该任务".into());
    }
    let card = crate::collab::tasks::submit(tid, i_of(v, "prId"), &ctx.username)?;
    emit_task(st, &card);
    // 触发本轮检查(后台线程,不阻塞提交响应;结果经 collab:check 推送)。
    spawn_checks(st, &card);
    ok(card)
});

/// 后台线程跑一轮检查(提交触发/手动重跑共用):结果落 check_runs,进度经 collab:check 推送。
pub(super) fn spawn_checks(st: &CollabState, card: &crate::collab::tasks::TaskCard) {
    let card2 = card.clone();
    let app2 = st.app.clone();
    std::thread::spawn(move || {
        let profile = crate::collab::checks::project_profile(card2.project_id);
        let repo = match project_repo_path(card2.project_id) {
            Ok(r) => r,
            Err(_) => return, // 项目没配仓库 → 无从检查,静默跳过
        };
        let emit = || {
            let _ = app2.emit(
                "collab:check",
                serde_json::json!({"taskId": card2.id, "round": card2.round}),
            );
        };
        let _ = crate::collab::checks::run_for_task(
            &repo,
            &card2.branch,
            card2.id,
            card2.round,
            &profile,
            &emit,
        );
    });
}

task_op!(task_review, |tid, ctx, v, st| {
    // 验收权=项目/团队管理者(协作者不能给自己验收)。asLead 也只能由管理者触发——
    // 落档 actor 记 lead:<expert>,且额外过 lead.rs 三问(任命/授权位/预算)。
    let card = crate::collab::tasks::get(tid)?;
    if !crate::collab::projects::can_admin(card.project_id, ctx.user_id, role_rank(&ctx.role) >= 3)
    {
        return Err("只有项目/团队管理者能验收".into());
    }
    if card.assignee == Some(ctx.user_id) {
        return Err("任务负责人不能验收自己提交的任务，请由另一位项目管理者复核".into());
    }
    let pass = v.get("pass").and_then(|x| x.as_bool()).ok_or("缺 pass")?;
    let comments = v
        .get("comments")
        .map(|c| c.to_string())
        .unwrap_or_else(|| "[]".into());
    let as_lead = v.get("asLead").and_then(|x| x.as_bool()).unwrap_or(false);
    let outc = if as_lead {
        crate::collab::lead::lead_review(card.project_id, tid, pass, &comments)?
    } else {
        crate::collab::tasks::review(tid, &ctx.username, pass, &comments)?
    };
    emit_task(st, &outc.card);
    if outc.escalated {
        // 打回熔断:抄送 owner(飞书通道接入前先走面板事件)。
        let _ = st.app.emit(
            "collab:escalate",
            serde_json::json!({"taskId": tid, "round": outc.card.round}),
        );
    }
    ok(outc)
});

task_op!(task_archive, |tid, ctx, _v, st| {
    let before = crate::collab::tasks::get(tid)?;
    if !crate::collab::projects::can_admin(
        before.project_id,
        ctx.user_id,
        role_rank(&ctx.role) >= 3,
    ) {
        return Err("只有项目管理者能归档任务".into());
    }
    let card = crate::collab::tasks::archive(tid, &ctx.username)?;
    emit_task(st, &card);
    ok(card)
});

task_op!(task_cancel, |tid, ctx, _v, st| {
    let before = crate::collab::tasks::get(tid)?;
    if !crate::collab::projects::can_admin(
        before.project_id,
        ctx.user_id,
        role_rank(&ctx.role) >= 3,
    ) {
        return Err("只有项目管理者能取消任务".into());
    }
    let card = crate::collab::tasks::cancel(tid, &ctx.username)?;
    emit_task(st, &card);
    ok(card)
});

pub(super) async fn task_rounds(
    State(state): State<CollabState>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    let Some(tid) = q.get("taskId").and_then(|s| s.parse::<i64>().ok()) else {
        return (StatusCode::BAD_REQUEST, Json(json!({"error":"缺 taskId"}))).into_response();
    };
    let pid = match tokio::task::spawn_blocking(move || {
        crate::collab::tasks::get(tid).map(|c| c.project_id)
    })
    .await
    {
        Ok(Ok(p)) => p,
        Ok(Err(e)) => return (StatusCode::BAD_REQUEST, Json(json!({"error": e}))).into_response(),
        Err(e) => return err_resp(e.to_string()),
    };
    if let Err(r) = ensure_member(&ctx, pid) {
        return r;
    }
    let out =
        tokio::task::spawn_blocking(move || crate::collab::tasks::rounds(tid).and_then(ok)).await;
    unwrap_api(out)
}

/// 项目动态时间线(GitHub activity feed 式,项目主页概览 tab 数据源)。
pub(super) async fn collab_activity(
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
    let limit = q
        .get("limit")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(30)
        .clamp(1, 100);
    let out = tokio::task::spawn_blocking(move || {
        crate::collab::tasks::activity(pid, limit).and_then(ok)
    })
    .await;
    unwrap_api(out)
}

pub(super) use task_op;

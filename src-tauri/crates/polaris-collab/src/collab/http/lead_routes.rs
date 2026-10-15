use super::*;

// ── 检查工作流(CI-lite,GitHub status checks 式)──

/// GET /api/collab/checks?taskId= → {profile, round, runs}(round 取卡当前轮)。
pub(super) async fn checks_get(
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
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        crate::collab::checks::sweep_stale_running(); // 崩溃残留的 running 行超时清掉,前端不永久转圈
        let card = crate::collab::tasks::get(tid)?;
        let profile = crate::collab::checks::project_profile(card.project_id);
        let check_skill = crate::collab::checks::project_check_skill(card.project_id);
        // 用检查实际落库的最大轮次,而非 card.round(review 通过会 +1 令其漂移;见
        // checks::latest_round)。没跑过检查则回落 card.round(runs 为空,前端不显示徽章)。
        let rnd = crate::collab::checks::latest_round(tid).unwrap_or(card.round);
        let runs = crate::collab::checks::list(tid, rnd)?;
        Ok(json!({"profile": profile, "checkSkill": check_skill, "round": rnd, "runs": runs}))
    })
    .await;
    unwrap_api(out)
}

// 手动重跑本轮检查(成员即可;复用提交触发的后台线程逻辑)。
task_op!(checks_rerun, |tid, _ctx, _v, st| {
    let card = crate::collab::tasks::get(tid)?;
    if card.branch.is_empty() {
        return Err("任务尚未开分支,无从检查".into());
    }
    spawn_checks(st, &card);
    ok(json!({"ok": true}))
});

/// 检查档位设置(code/creative/off)——项目/团队管理者专属。
pub(super) async fn checks_profile_set(
    State(state): State<CollabState>,
    headers: HeaderMap,
    Json(v): Json<Value>,
) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    let global_owner = role_rank(&ctx.role) >= 3;
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let pid = i_of(&v, "projectId").ok_or("缺 projectId")?;
        if !crate::collab::projects::can_admin(pid, ctx.user_id, global_owner) {
            return Err("只有项目/团队管理者能改检查档位".into());
        }
        let profile = s_of(&v, "profile");
        if !profile.is_empty() {
            crate::collab::checks::set_project_profile(pid, &profile)?;
            crate::collab::db::audit(&ctx.username, "checks.profile", &pid.to_string(), &profile);
        }
        // 可选:同请求顺带设检查技能(前端设置面板一次提交)。传空串=回到默认内置技能。
        if let Some(skill) = v.get("checkSkill").and_then(|s| s.as_str()) {
            crate::collab::checks::set_project_check_skill(pid, skill)?;
            crate::collab::db::audit(&ctx.username, "checks.skill", &pid.to_string(), skill);
        }
        Ok(json!({"ok": true}))
    })
    .await;
    unwrap_api(out)
}

/// 主机本机已安装、可用作检查项的技能清单(检查设置下拉)。
pub(super) async fn checks_skills_list(State(state): State<CollabState>, headers: HeaderMap) -> Response {
    let Some(_ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let items: Vec<Value> = crate::skills::list_check_capable()
            .into_iter()
            .map(|(id, name)| json!({"id": id, "name": name}))
            .collect();
        Ok(json!({"skills": items, "default": crate::skills::PROJECT_CHECK_ID}))
    })
    .await;
    unwrap_api(out)
}

// ── 主 Agent 授权位 + 晨会 ──

pub(super) async fn lead_grants_get(
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
        tokio::task::spawn_blocking(move || crate::collab::lead::get_grants(pid).and_then(ok))
            .await;
    unwrap_api(out)
}

/// 授权位只有项目/团队管理者能改——这是"owner 可随时收权/降档"的落点。
pub(super) async fn lead_grants_set(
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
            return Err("只有项目/团队管理者能改主 Agent 授权位".into());
        }
        let g: crate::collab::lead::LeadGrants =
            serde_json::from_value(v.get("grants").cloned().ok_or("缺 grants")?)
                .map_err(|e| e.to_string())?;
        crate::collab::lead::set_grants(pid, &g, &ctx.username).map(|_| json!({"ok": true}))
    })
    .await;
    unwrap_api(out)
}

pub(super) async fn lead_morning(
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
        tokio::task::spawn_blocking(move || crate::collab::lead::morning_report(pid).and_then(ok))
            .await;
    unwrap_api(out)
}

/// 主 Agent 改派(指挥件②):管理者触发,真正的授权判定在 lead.rs 三问(须 can_reassign 位)。
pub(super) async fn lead_assign_api(
    State(state): State<CollabState>,
    headers: HeaderMap,
    Json(v): Json<Value>,
) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    let st = state.clone();
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let tid = i_of(&v, "taskId").ok_or("缺 taskId")?;
        let uid = i_of(&v, "userId").ok_or("缺 userId")?;
        let card = crate::collab::tasks::get(tid)?;
        if !crate::collab::projects::can_admin(
            card.project_id,
            ctx.user_id,
            role_rank(&ctx.role) >= 3,
        ) {
            return Err("只有项目/团队管理者能触发主 Agent 改派".into());
        }
        let card = crate::collab::lead::lead_assign(card.project_id, tid, uid)?;
        emit_task(&st, &card);
        ok(card)
    })
    .await;
    unwrap_api(out)
}

/// 主 Agent 催办(指挥件⑥):盘出超期无动静的卡并留痕,返回催办清单。
pub(super) async fn lead_nudge_api(
    State(state): State<CollabState>,
    headers: HeaderMap,
    Json(v): Json<Value>,
) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let pid = i_of(&v, "projectId").ok_or("缺 projectId")?;
        if !crate::collab::projects::can_admin(pid, ctx.user_id, role_rank(&ctx.role) >= 3) {
            return Err("只有项目/团队管理者能触发催办".into());
        }
        let hours = i_of(&v, "staleHours").unwrap_or(48).max(1);
        crate::collab::lead::lead_nudge(pid, hours).and_then(ok)
    })
    .await;
    unwrap_api(out)
}

// ── 主 Agent AI ──

pub(super) async fn lead_model_get(State(state): State<CollabState>, headers: HeaderMap) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    if role_rank(&ctx.role) < 3 {
        return forbid();
    }
    let mut cfg = crate::collab::lead_ai::load_cfg();
    // 密钥脱敏:只回是否已配置。
    if !cfg.api_key.is_empty() {
        cfg.api_key = "•••".into();
    }
    Json(serde_json::to_value(cfg).unwrap_or_default()).into_response()
}

pub(super) async fn lead_model_set(
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
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let mut cfg: crate::collab::lead_ai::LeadModelCfg =
            serde_json::from_value(v).map_err(|e| e.to_string())?;
        // 前端传 ••• 表示保留原密钥。
        if cfg.api_key == "•••" {
            cfg.api_key = crate::collab::lead_ai::load_cfg().api_key;
        }
        crate::collab::lead_ai::save_cfg(&cfg)?;
        Ok(json!({"ok": true}))
    })
    .await;
    unwrap_api(out)
}

/// 拆卡:goal → 草案;dispatch=true 且授权允许时直接建卡(仍是四要素强制)。
pub(super) async fn lead_ai_decompose(
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
    let st = state.clone();
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let pid = i_of(&v, "projectId").ok_or("缺 projectId")?;
        if !crate::collab::projects::can_admin(pid, ctx.user_id, role_rank(&ctx.role) >= 3) {
            return Err("只有项目管理者能调用主 Agent".into());
        }
        let _run_guard = LeadAiRunGuard::acquire(pid)?;
        let drafts =
            crate::collab::lead_ai::ai_decompose(pid, &s_of(&v, "goal"), &s_of(&v, "memberHint"))?;
        let dispatch = v.get("dispatch").and_then(|x| x.as_bool()).unwrap_or(false)
            && crate::collab::lead::get_grants(pid)?.auto_dispatch;
        let mut created = Vec::new();
        if dispatch {
            for d in &drafts {
                if d.title.starts_with("待澄清") {
                    continue; // 拆不动的不硬拆,留给 owner
                }
                if let Ok(card) = crate::collab::lead::lead_create_task(
                    pid,
                    &d.title,
                    &d.body,
                    &d.scope,
                    &d.criteria,
                ) {
                    emit_task(&st, &card);
                    created.push(card);
                }
            }
        }
        Ok(json!({"drafts": drafts, "created": created}))
    })
    .await;
    unwrap_api(out)
}

/// 验收草稿:自动取任务分支相对 main 的 diff 喂给模型,产出意见草稿(不落状态机)。
pub(super) async fn lead_ai_review(
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
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let tid = i_of(&v, "taskId").ok_or("缺 taskId")?;
        let card = crate::collab::tasks::get(tid)?;
        if !crate::collab::projects::can_admin(
            card.project_id,
            ctx.user_id,
            role_rank(&ctx.role) >= 3,
        ) {
            return Err("只有项目管理者能调用主 Agent".into());
        }
        let _run_guard = LeadAiRunGuard::acquire(card.project_id)?;
        let repo = project_repo_path(card.project_id)?;
        let diff_text = git_diff_limited(&repo, &format!("main...{}", card.branch))?;
        let draft = crate::collab::lead_ai::ai_review(card.project_id, tid, &diff_text)?;
        ok(draft)
    })
    .await;
    unwrap_api(out)
}

/// 冲突融合草案(指挥件④的 AI 侧):对单个冲突块起草融合文本。
/// 只产草案不落任何东西——落地须经 merge/resolve(人工确认后先落 PR 分支)。
pub(super) async fn lead_ai_fuse(
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
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let tid = i_of(&v, "taskId").ok_or("缺 taskId")?;
        let file = s_of(&v, "file");
        let block_idx = i_of(&v, "blockIndex").ok_or("缺 blockIndex")? as usize;
        if file.is_empty() {
            return Err("缺 file".into());
        }
        let card = crate::collab::tasks::get(tid)?;
        if !crate::collab::projects::can_admin(
            card.project_id,
            ctx.user_id,
            role_rank(&ctx.role) >= 3,
        ) {
            return Err("只有项目管理者能调用主 Agent".into());
        }
        let _run_guard = LeadAiRunGuard::acquire(card.project_id)?;
        let repo = project_repo_path(card.project_id)?;
        let blocks = crate::collab::mergectl::conflict_blocks(&repo, "main", &card.branch, &file)?;
        let b = blocks
            .get(block_idx)
            .ok_or("冲突块不存在(态势可能已变化,请刷新)")?;
        let context = format!(
            "文件 {file} 第 {}–{} 行,任务卡:{}",
            b.start_line, b.end_line, card.title
        );
        let text = crate::collab::lead_ai::ai_fuse(card.project_id, &b.ours, &b.theirs, &context)?;
        Ok(json!({"text": text}))
    })
    .await;
    unwrap_api(out)
}

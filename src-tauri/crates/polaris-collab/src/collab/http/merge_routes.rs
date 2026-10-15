use super::*;

// ── 合并闸门 ──
//
// 三级闸(v8 5.3):① 机器闸=merge_trial 干净 + 提交者是项目成员;② 验收闸=卡上
// 最新一轮 verdict=pass;③ 放行闸=owner 亲点,或主 Agent 有 can_merge 位。

pub(super) fn configured_repo_root() -> Result<PathBuf, String> {
    let root = if let Ok(raw) = std::env::var("POLARIS_REPO_ROOT") {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err("POLARIS_REPO_ROOT 不能为空".into());
        }
        PathBuf::from(trimmed)
    } else {
        directories::UserDirs::new()
            .ok_or("无法确定用户目录，请设置 POLARIS_REPO_ROOT")?
            .home_dir()
            .join("Polaris")
            .join("repos")
    };
    std::fs::create_dir_all(&root).map_err(|e| format!("创建仓库根目录失败: {e}"))?;
    std::fs::canonicalize(&root).map_err(|e| format!("解析仓库根目录失败: {e}"))
}

/// 只接受管理员预先放入 POLARIS_REPO_ROOT 的本地 Git 仓库。canonicalize 后再比对，
/// 同时阻断 `..` 与目录符号链接逃逸到宿主机其它位置。
pub(super) fn validate_repo_path(repo: &str) -> Result<PathBuf, String> {
    let raw = repo.trim();
    if raw.is_empty() {
        return Err("项目仓库路径不能为空".into());
    }
    let path = Path::new(raw);
    if !path.is_absolute() {
        return Err("仓库必须填写宿主机绝对路径，不能填写 Git 远程 URL".into());
    }
    let canonical =
        std::fs::canonicalize(path).map_err(|e| format!("仓库路径不存在或不可访问: {e}"))?;
    let root = configured_repo_root()?;
    if !canonical.starts_with(&root) {
        return Err(format!(
            "仓库必须位于受控目录 {} 内（可用 POLARIS_REPO_ROOT 修改）",
            root.display()
        ));
    }
    if !canonical.join(".git").exists() && !canonical.join("HEAD").is_file() {
        return Err(format!("仓库路径不是 Git 仓库: {}", canonical.display()));
    }
    Ok(canonical)
}

/// `Command::output()` 会在内存里无上限收完整 diff，巨大仓库可直接 OOM；这里边读边限到
/// 1MiB，并给 git 20 秒硬超时。超过上限后立即杀掉子进程，既保护内存也保护模型预算。
pub(super) fn git_diff_limited(repo: &Path, revision: &str) -> Result<String, String> {
    use std::io::Read as _;
    use std::process::Stdio;
    use std::sync::mpsc::{self, TryRecvError};

    const LIMIT: usize = 1024 * 1024;
    let mut child = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["diff", "--unified=3", revision])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("取 diff 失败: {e}"))?;
    let mut stdout = child.stdout.take().ok_or("无法读取 git diff 输出")?;
    let (tx, rx) = mpsc::channel::<Result<Vec<u8>, String>>();
    let reader = std::thread::spawn(move || {
        let mut out = Vec::with_capacity(64 * 1024);
        let mut buf = [0u8; 64 * 1024];
        loop {
            match stdout.read(&mut buf) {
                Ok(0) => {
                    let _ = tx.send(Ok(out));
                    return;
                }
                Ok(n) if out.len().saturating_add(n) <= LIMIT => out.extend_from_slice(&buf[..n]),
                Ok(_) => {
                    let _ = tx.send(Err(
                        "变更超过 1MB，拒绝整包送入模型；请缩小任务范围或拆分任务".into(),
                    ));
                    return;
                }
                Err(e) => {
                    let _ = tx.send(Err(format!("读取 diff 失败: {e}")));
                    return;
                }
            }
        }
    });

    let started = std::time::Instant::now();
    let mut captured: Option<Result<Vec<u8>, String>> = None;
    loop {
        if captured.is_none() {
            match rx.try_recv() {
                Ok(result) => {
                    if result.is_err() {
                        let _ = child.kill();
                    }
                    captured = Some(result);
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = reader.join();
                    return Err("读取 git diff 的工作线程异常退出".into());
                }
            }
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|e| format!("等待 git diff 失败: {e}"))?
        {
            let result = match captured {
                Some(result) => result,
                None => rx
                    .recv_timeout(std::time::Duration::from_secs(2))
                    .map_err(|_| "读取 git diff 超时".to_string())?,
            };
            let _ = reader.join();
            if !status.success() {
                return Err("取 diff 失败，请确认 main 与任务分支存在".into());
            }
            return result.map(|bytes| String::from_utf8_lossy(&bytes).into_owned());
        }
        if started.elapsed() >= std::time::Duration::from_secs(20) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            return Err("取 diff 超过 20 秒，已终止；请缩小任务范围".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// 项目仓库的本地路径(projects.repo 存主机侧权威仓库路径)。每次使用都重新校验，
/// 让旧版本创建的越界记录也无法继续触达任意宿主机仓库。
pub fn project_repo_path(project_id: i64) -> Result<std::path::PathBuf, String> {
    let p = crate::collab::projects::get(project_id)?;
    validate_repo_path(&p.repo)
}

/// 冲突试算(无副作用,任何成员可跑)。入参 taskId;分支取卡上记录。
pub(super) async fn merge_trial_api(
    State(state): State<CollabState>,
    headers: HeaderMap,
    Json(v): Json<Value>,
) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    // 注意不能按全局角色早拒(role_rank<2):全局 visitor 可以是团队/项目管理者
    // (can_admin 认项目成员表),早拒会把合法管理者挡在裁决台数据源之外。
    // 资源面由下面的项目成员闸兜住:非成员(含普通 visitor)照旧 403。
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let tid = i_of(&v, "taskId").ok_or("缺 taskId")?;
        let card = crate::collab::tasks::get(tid)?;
        if role_rank(&ctx.role) < 3
            && crate::collab::projects::member_role(card.project_id, ctx.user_id).is_none()
        {
            return Err("你不是该项目成员".into());
        }
        if card.branch.is_empty() {
            return Err("任务尚未开分支".into());
        }
        let repo = project_repo_path(card.project_id)?;
        let trial = crate::collab::mergectl::merge_trial(&repo, "main", &card.branch)?;
        let (behind, ahead) = crate::collab::mergectl::behind_count(&repo, "main", &card.branch)?;
        let overlap =
            crate::collab::mergectl::scope_overlap(&repo, "main", &card.branch, &card.scope)?;
        // 冲突时顺带给出每个文件的结构化冲突块(裁决台数据源)。
        let mut blocks = serde_json::Map::new();
        if !trial.clean {
            for f in trial.conflict_files.iter().take(20) {
                if let Ok(b) =
                    crate::collab::mergectl::conflict_blocks(&repo, "main", &card.branch, f)
                {
                    blocks.insert(f.clone(), serde_json::to_value(b).unwrap_or(Value::Null));
                }
            }
        }
        Ok(json!({
            "clean": trial.clean,
            "conflictFiles": trial.conflict_files,
            "behind": behind, "ahead": ahead,
            "scopeOverlap": overlap,
            "conflictBlocks": blocks,
        }))
    })
    .await;
    unwrap_api(out)
}

/// 冲突裁决落地:逐块处置(采纳某侧/融合草案)一次性落成任务分支上的合并提交。
/// 管理者专属——融合草案(含 AI 起草)必须由人在这里确认才会落分支,且永不直写 main。
pub(super) async fn merge_resolve_api(
    State(state): State<CollabState>,
    headers: HeaderMap,
    Json(v): Json<Value>,
) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let tid = i_of(&v, "taskId").ok_or("缺 taskId")?;
        let card = crate::collab::tasks::get(tid)?;
        if !crate::collab::projects::can_admin(
            card.project_id,
            ctx.user_id,
            role_rank(&ctx.role) >= 3,
        ) {
            return Err("只有项目/团队管理者能落地裁决".into());
        }
        if card.state != "review" {
            return Err("任务不在待验收状态,不能裁决".into());
        }
        if card.branch.is_empty() {
            return Err("任务尚未开分支".into());
        }
        let resolutions: std::collections::HashMap<
            String,
            Vec<crate::collab::mergectl::BlockResolution>,
        > = serde_json::from_value(v.get("resolutions").cloned().ok_or("缺 resolutions")?)
            .map_err(|e| format!("resolutions 结构不符: {e}"))?;
        let repo = project_repo_path(card.project_id)?;
        let oid = crate::collab::mergectl::resolve_conflicts(
            &repo,
            "main",
            &card.branch,
            &resolutions,
            &ctx.username,
        )?;
        Ok(json!({"ok": true, "commit": oid}))
    })
    .await;
    unwrap_api(out)
}

/// squash 合并放行:走满三级闸,合并后推进状态机+广播。
pub(super) async fn merge_squash_api(
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
        let card = crate::collab::tasks::get(tid)?;
        // 放行闸:owner 直通;主 Agent 路径走 lead_approve_merge(查 can_merge 位)。
        // asLead 与 task_review 同一铁律:也只能由项目/团队管理者代为发起——否则任意
        // 登录账号(含 visitor)都能借 asLead 抢先放行合并,且落痕记成 lead 误导审计。
        let as_lead = v.get("asLead").and_then(|x| x.as_bool()).unwrap_or(false);
        let can_admin = crate::collab::projects::can_admin(card.project_id, ctx.user_id, role_rank(&ctx.role) >= 3);
        if !can_admin {
            return Err("合并放行需要项目/团队管理者(主 Agent 路径也须由管理者代为发起)".into());
        }
        let actor = if as_lead {
            crate::collab::lead::lead_approve_merge(card.project_id, tid)?;
            format!("lead(申请人:{})", ctx.username)
        } else {
            ctx.username.clone()
        };
        // 验收闸:最新一轮必须 pass。
        let rounds = crate::collab::tasks::rounds(tid)?;
        if rounds.last().map(|r| r.verdict.as_str()) != Some("pass") {
            return Err("验收闸未过:最新一轮验收不是通过".into());
        }
        if card.state != "review" {
            return Err("任务不在待验收状态".into());
        }
        // 先解析仓库:没配仓库时给出真实原因,而不是误报「检查闸未过」。
        let repo = project_repo_path(card.project_id)?;
        // 检查闸(GitHub required checks 式):最新一轮检查须全绿,且**分支头没变**
        // (检查过后又推新提交=陈旧检查;GitHub 按 commit 查,我们按轮+SHA 补齐语义)。
        // profile=off 不拦;owner 可 force 强推(留痕审计)。
        // force 只认 can_admin 分支——as_lead 路径不许 force(主 Agent 无权跳检查)。
        let force = can_admin && !as_lead && v.get("force").and_then(|x| x.as_bool()).unwrap_or(false);
        let profile = crate::collab::checks::project_profile(card.project_id);
        if profile != "off" && !force {
            // 先扫僵尸 running(进程崩过留下的),否则那张卡永远卡在「未跑完」合不了。
            crate::collab::checks::sweep_stale_running();
            // 按「检查实际落库的最大轮次」判定,而非 card.round —— review 通过会 +1 使 card.round
            // 漂到检查记录之上(见 checks::latest_round 注释)。None=从没跑过检查 → 视为未过。
            let crnd = match crate::collab::checks::latest_round(tid) {
                Some(r) => r,
                None => return Err("检查闸未过:尚未跑过检查,请提交送验触发检查".into()),
            };
            match crate::collab::checks::all_green(tid, crnd) {
                Ok(true) => {
                    // SHA 陈旧比对:记录为空(老数据)放行兼容;比对不上要求重跑。
                    if let Some(checked_sha) = crate::collab::checks::round_sha(tid, crnd) {
                        let head = std::process::Command::new("git")
                            .arg("-C").arg(&repo)
                            .args(["rev-parse", &card.branch])
                            .output()
                            .ok()
                            .filter(|o| o.status.success())
                            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
                        if let Some(head) = head {
                            if head != checked_sha {
                                return Err("检查闸未过:检查之后分支又有新提交(检查已陈旧),请重跑检查或重新送验".into());
                            }
                        }
                    }
                }
                Ok(false) => return Err("检查闸未过:本轮检查未全绿(或未跑完)。owner 可带 force 强推".into()),
                // 查询出错必须 fail-closed:SQLite busy 等偶发错误不能被当成「通过」放行。
                Err(e) => return Err(format!("检查闸未过:检查结果查询失败({e}),请重试")),
            }
        }
        if force {
            crate::collab::db::audit(&ctx.username, "merge.force", &tid.to_string(), "跳过检查闸强推");
        }
        // 机器闸 + 执行(squash_merge 内部会重跑试算,不干净即拒)。
        let title = format!("#{} {}", card.id, card.title);
        let oid = crate::collab::mergectl::squash_merge(&repo, "main", &card.branch, &title, &actor)?;
        // 合并已落 main,此后状态机推进失败不能当普通 400 吞掉——那会让仓库与卡状态
        // 永久不一致且无人知晓。留审计并明确告知需人工核对。
        let merged = crate::collab::tasks::mark_merged(tid, &actor).map_err(|e| {
            // 双保险留痕:audit 与 mark_merged 同库,库级故障(只读/满盘)时 audit 也会
            // 静默失败——stderr 是唯一不依赖该库的持久痕迹(容器/服务日志可查)。
            eprintln!("[collab] merge.state_drift task={tid} commit={oid} err={e}");
            crate::collab::db::audit(
                &actor,
                "merge.state_drift",
                &tid.to_string(),
                &format!("squash 已落 main({oid}) 但状态机推进失败: {e}"),
            );
            format!("合并已落地(提交 {oid}),但任务状态更新失败: {e}。请手动核对该卡状态")
        })?;
        emit_task(&st, &merged);
        Ok(json!({"ok": true, "commit": oid, "card": merged}))
    })
    .await;
    unwrap_api(out)
}

/// 以卡回滚:revert 一个 squash 提交=整卡撤销。项目/团队管理者专属(主 Agent 只能"申请")。
pub(super) async fn merge_revert_api(
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
            return Err("只有项目/团队管理者能回滚".into());
        }
        let oid = s_of(&v, "commit");
        if oid.is_empty() {
            return Err("缺 commit".into());
        }
        let repo = project_repo_path(pid)?;
        let new_oid = crate::collab::mergectl::revert_card(&repo, &oid, &ctx.username)?;
        Ok(json!({"ok": true, "commit": new_oid}))
    })
    .await;
    unwrap_api(out)
}

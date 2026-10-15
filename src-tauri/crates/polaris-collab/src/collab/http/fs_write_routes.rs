use super::*;

// ───────────────────────── 远程文件写入(挂载盘的读写面)─────────────────────────
//
// 写比读危险一个量级 —— 远端能删你的文件。所以这批端点有**两道**独立的闸:
//  ① 角色闸:与读同一条线(owner 级)。挂载端拿的是同账号断言换来的本机会话。
//  ② 目录闸:fsface 逐根的写位。角色够了但目录没开写 → 一律拒。
// 两道都得过。少任何一道,「同账号自动组网」就变成「谁进得来谁就能删盘」。

/// 写端点共用的入口闸:角色或设备契约(见 [`fs_allowed`])。
/// 拒绝原因原样带出来 —— 「对方把这台设备设成了只读」必须显示成失败原因,
/// 糊成一句「需要 owner 权限」会让人对着正确的操作反复试。
pub(super) async fn fs_write_guard(state: &CollabState, headers: &HeaderMap) -> Result<AuthCtx, String> {
    let ctx = auth_ctx(state, headers)
        .await
        .ok_or_else(|| "未授权".to_string())?;
    fs_allowed(&ctx, true)?;
    Ok(ctx)
}

/// fsface 的错误 → 403 + 原文。写失败的原因(只读/不在根内/父目录不存在)必须如实回给
/// 挂载端,资源管理器会把它显示成失败原因;糊成 500 就只剩一句「未指定的错误」。
pub(super) fn fs_deny(e: String) -> Response {
    (StatusCode::FORBIDDEN, Json(json!({ "error": e }))).into_response()
}

/// 单个条目属性。挂载盘的 PROPFIND(文件)与 HEAD 走这条,省掉「列父目录再找一遍」。
pub(super) async fn fs_stat_api(
    State(state): State<CollabState>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    if let Err(e) = fs_allowed(&ctx, false) {
        return fs_deny(e);
    }
    let rel = q.get("path").cloned().unwrap_or_default();
    match tokio::task::spawn_blocking(move || crate::collab::fsface::stat(&rel)).await {
        Ok(Ok(e)) => Json(json!({ "entry": e })).into_response(),
        Ok(Err(e)) => fs_deny(e),
        Err(e) => err_resp(format!("内部任务失败: {e}")),
    }
}

/// 本机对远端开放了什么能力。挂载端据此决定盘挂成读写还是只读 ——
/// 不问就盲挂读写,用户会在资源管理器里拖进文件、等半天、再收一个「拒绝访问」。
pub(super) async fn fs_caps_api(State(state): State<CollabState>, headers: HeaderMap) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    if let Err(e) = fs_allowed(&ctx, false) {
        return fs_deny(e);
    }
    // 写位要按**这台设备**报:契约给了只读,就别对它宣称可写 —— 挂载端信了 caps
    // 会把盘挂成读写,用户拖进文件、等半天、再收一个「拒绝访问」。
    let writable = fs_allowed(&ctx, true).is_ok();
    let out = tokio::task::spawn_blocking(move || {
        let (read, write) = crate::collab::fsface::caps();
        json!({ "read": read, "write": write && writable })
    })
    .await;
    match out {
        Ok(v) => Json(v).into_response(),
        Err(e) => err_resp(format!("内部任务失败: {e}")),
    }
}

/// 写一个文件(整体覆盖)。请求体**流式**落到临时文件再原子改名:
/// 4GB 的文件不会在内存里过一遍,半途断线也不会留下一个截断的正式文件。
pub(super) async fn fs_write_api(
    State(state): State<CollabState>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
    body: Body,
) -> Response {
    let ctx = match fs_write_guard(&state, &headers).await {
        Ok(c) => c,
        Err(e) => return fs_deny(e),
    };
    let rel = q.get("path").cloned().unwrap_or_default();
    // 开写(建临时文件)在阻塞线程池;拿到句柄后由 mpsc 把 async 流喂给同步写线程。
    let opened = tokio::task::spawn_blocking({
        let rel = rel.clone();
        move || crate::collab::fsface::begin_write(&rel)
    })
    .await;
    let (file, target) = match opened {
        Ok(Ok(t)) => t,
        Ok(Err(e)) => return fs_deny(e),
        Err(e) => return err_resp(format!("内部任务失败: {e}")),
    };

    let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(4);
    // 同步写线程:收一块写一块。返回写入字节数;写盘出错立刻停,错误带回调用侧。
    let writer = tokio::task::spawn_blocking(move || -> Result<u64, String> {
        use std::io::Write;
        let mut file = file;
        let mut total: u64 = 0;
        while let Ok(chunk) = rx.recv() {
            file.write_all(&chunk)
                .map_err(|e| format!("写盘失败: {e}"))?;
            total += chunk.len() as u64;
        }
        file.flush().map_err(|e| format!("落盘失败: {e}"))?;
        Ok(total)
    });

    let mut stream = body.into_data_stream();
    let mut recv_err: Option<String> = None;
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(b) => {
                if tx.send(b.to_vec()).is_err() {
                    break; // 写线程已经出错退出,别再灌了
                }
            }
            Err(e) => {
                recv_err = Some(format!("接收请求体中断: {e}"));
                break;
            }
        }
    }
    drop(tx); // 关通道 = 通知写线程收尾
    let written = writer.await;

    // 任一环出错都不落定:临时文件删掉,正式文件保持原样。
    let cleanup = |t: crate::collab::fsface::WriteTarget| {
        tokio::task::spawn_blocking(move || crate::collab::fsface::abort_write(&t));
    };
    if let Some(e) = recv_err {
        cleanup(target);
        return err_resp(e);
    }
    let total = match written {
        Ok(Ok(n)) => n,
        Ok(Err(e)) => {
            cleanup(target);
            return err_resp(e);
        }
        Err(e) => {
            cleanup(target);
            return err_resp(format!("写任务失败: {e}"));
        }
    };
    let committed = tokio::task::spawn_blocking(move || {
        crate::collab::fsface::commit_write(&target).map_err(|e| {
            crate::collab::fsface::abort_write(&target);
            e
        })
    })
    .await;
    match committed {
        Ok(Ok(())) => {
            crate::collab::db::audit(&ctx.username, "fs.write", &rel, &total.to_string());
            Json(json!({ "ok": true, "bytes": total })).into_response()
        }
        Ok(Err(e)) => err_resp(e),
        Err(e) => err_resp(format!("落定任务失败: {e}")),
    }
}

/// 建目录 / 删除 / 改名 / 复制。都是短活,统一走一个 handler 少四份样板。
pub(super) async fn fs_op_api(
    State(state): State<CollabState>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
    axum::extract::Path(op): axum::extract::Path<String>,
) -> Response {
    let ctx = match fs_write_guard(&state, &headers).await {
        Ok(c) => c,
        Err(e) => return fs_deny(e),
    };
    let rel = q.get("path").cloned().unwrap_or_default();
    let dest = q.get("dest").cloned().unwrap_or_default();
    let op2 = op.clone();
    let rel2 = rel.clone();
    let dest2 = dest.clone();
    let out = tokio::task::spawn_blocking(move || -> Result<(), String> {
        use crate::collab::fsface as F;
        match op2.as_str() {
            "mkdir" => F::mkdir(&rel2),
            "delete" => F::remove(&rel2),
            "move" => F::rename(&rel2, &dest2),
            "copy" => F::copy(&rel2, &dest2),
            other => Err(format!("不认识的文件操作「{other}」")),
        }
    })
    .await;
    match out {
        Ok(Ok(())) => {
            crate::collab::db::audit(&ctx.username, &format!("fs.{op}"), &rel, &dest);
            Json(json!({ "ok": true })).into_response()
        }
        Ok(Err(e)) => fs_deny(e),
        Err(e) => err_resp(format!("内部任务失败: {e}")),
    }
}

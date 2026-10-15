use super::*;

// ───────────────────────── 同账号设备网 /api/mesh/*(权威侧)─────────────────────────
//
// 「登录同一个账号 → 设备自己成网」的目录服务。只有账号权威(云机)提供;成员主机上这些
// 路由一律 400,与 /api/account/* 同口径。
//
// 鉴权是**设备密钥**(不是会话):Bearer mk_… 由 /api/mesh/enroll 颁发。理由是设备网的
// 心跳跑在后台、跨天跨重启,不该依赖某个会话的寿命;而它能做的事只有三样(报到、取本账号
// 设备清单、换一张 5 分钟身份断言),换不到任何一台设备的访问权。

/// 设备密钥入口闸。密钥无效/被吊销 → 403 并带上原因(客户端据此决定「重新入网」)。
// ───────────────────────── 设备信任契约 /api/peer/*(每台主机本地)─────────────────────────
//
// 与上面 /api/mesh/* 的分工要分清,这是整套设计的关键一刀:
//  · /api/mesh/*  在**云端**,回答「我有哪些设备」——目录,不含任何权限。
//  · /api/peer/*  在**本机**,回答「这台设备在我这儿能干什么」——权限,云端永远碰不到。
// 云机被完全拿下也改不动任何一台机器的 peer_grants:它根本没有这张表。

/// 本机的设备信任契约清单(owner 才看得到:这是权限面)。
pub(super) async fn peer_grants_list(State(state): State<CollabState>, headers: HeaderMap) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    if role_rank(&ctx.role) < 3 {
        return forbid();
    }
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let list: Vec<Value> = crate::collab::grants::list()?
            .into_iter()
            .map(peer_grant_value)
            .collect();
        Ok(json!({ "grants": list, "ownerUid": crate::collab::authority::owner_uid() }))
    })
    .await;
    unwrap_api(out)
}

/// 改一台设备的档位(盘 none/ro/rw、执行 none/ask/allow、自动挂载)。owner 才行。
pub(super) async fn peer_grant_set(
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
        let opt = |k: &str| v.get(k).and_then(|x| x.as_str()).map(|s| s.to_string());
        let g = crate::collab::grants::update(
            &s_of(&v, "nodeId"),
            opt("name").as_deref(),
            opt("fsAccess").as_deref(),
            opt("execAccess").as_deref(),
            v.get("autoMount").and_then(|x| x.as_bool()),
            &ctx.username,
        )?;
        Ok(json!({ "ok": true, "effectiveFs": g.effective_fs(), "autoMount": g.auto_mount }))
    })
    .await;
    unwrap_api(out)
}

/// 「信任这台设备」:恢复被撤销的准入并升为全权。
pub(super) async fn peer_grant_trust(
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
        let g = crate::collab::grants::trust_fully(&s_of(&v, "nodeId"), &ctx.username)?;
        Ok(json!({ "ok": true, "effectiveFs": g.effective_fs() }))
    })
    .await;
    unwrap_api(out)
}

/// 撤销一台设备在**本机**的准入(丢了电脑 / 收回授权)。
///
/// 与云端的 `/api/mesh/revoke` 是两道独立的闸,应该都点:云端那道让它拿不到新断言,
/// 本机这道让它连断言都递不进来。云机被绕过、名册被伪造,本机这一行仍然说了算。
pub(super) async fn peer_grant_revoke(
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
        crate::collab::grants::revoke(&s_of(&v, "nodeId"), &ctx.username)?;
        Ok(json!({ "ok": true }))
    })
    .await;
    unwrap_api(out)
}

fn peer_grant_value(g: crate::collab::grants::Grant) -> Value {
    json!({
        "nodeId": g.node_id, "uid": g.uid, "name": g.name, "role": g.role,
        "fsAccess": g.fs_access, "execAccess": g.exec_access,
        // 实际生效的档包含撤销状态，UI 要显示这个,
        // 只显示库里那个会让用户以为「明明给了可写怎么写不进去」。
        "effectiveFs": g.effective_fs(), "effectiveExec": g.effective_exec(),
        "autoMount": g.auto_mount, "driveHint": g.drive_hint,
        "cooldownUntil": 0, "inCooldown": false,
        "grantedAt": g.granted_at, "grantedBy": g.granted_by, "revoked": g.revoked,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collab::{db, grants, interconnect::test_env::TestEnv};

    #[test]
    fn legacy_timestamp_never_overrides_permissions_or_reports_cooldown() -> Result<(), String> {
        let _env = TestEnv::new("legacy-grant")?;
        let peer = "node-legacy";
        grants::auto_grant_self(peer, "acct_test", "Legacy", false)?;
        db::open_db()?.execute(
            "UPDATE peer_grants SET cooldown_until=?1 WHERE node_id=?2",
            rusqlite::params![db::now() + 86400, peer],
        ).map_err(|e| e.to_string())?;
        assert!(grants::fs_gate(peer, true).is_ok());
        assert!(grants::exec_gate(peer).is_ok());
        let stored = grants::get(peer)?.ok_or("missing legacy grant")?;
        assert!(stored.cooldown_until > db::now());
        let wire = peer_grant_value(stored);
        assert_eq!(wire["inCooldown"], false);
        assert_eq!(wire["cooldownUntil"], 0);
        assert_eq!(wire["effectiveFs"], grants::FS_RW);

        grants::update(peer, None, Some(grants::FS_RO), Some(grants::EXEC_ASK), None, "owner")?;
        assert!(grants::fs_gate(peer, false).is_ok());
        let write_error = grants::fs_gate(peer, true).err().ok_or("write should be denied")?;
        assert!(write_error.contains("只读"));
        assert!(!write_error.contains("观察期"));
        let exec_error = grants::exec_gate(peer).err().ok_or("execution should be denied")?;
        assert!(exec_error.contains("每次问"));
        grants::revoke(peer, "owner")?;
        assert!(grants::fs_gate(peer, false).is_err());
        assert!(grants::exec_gate(peer).is_err());
        let revoked = peer_grant_value(grants::get(peer)?.ok_or("missing revoked grant")?);
        assert_eq!(revoked["effectiveFs"], grants::FS_NONE);
        assert_eq!(revoked["effectiveExec"], grants::EXEC_NONE);
        grants::trust_fully(peer, "owner")?;
        assert!(grants::fs_gate(peer, true).is_ok());
        assert!(grants::exec_gate(peer).is_ok());
        Ok(())
    }
}

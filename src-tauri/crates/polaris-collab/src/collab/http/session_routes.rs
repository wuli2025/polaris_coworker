use super::*;

/// owner 显式「重新信任」账号权威(换云机 / 权威轮换了密钥)。
/// 要 owner 亲自确认公钥指纹 —— 自动接受新公钥等于把验签这道防线拆了。
pub(super) async fn collab_authority_repin(
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
        let want = s_of(&v, "kid");
        let (url, pk) = crate::collab::authority::repin(if want.trim().is_empty() {
            None
        } else {
            Some(&want)
        })?;
        Ok(json!({"ok": true, "authorityUrl": url, "kid": crate::collab::authority::kid_of(&pk)}))
    })
    .await;
    unwrap_api(out)
}

pub(super) async fn collab_logout(headers: HeaderMap) -> Response {
    let token = bearer_of(&headers).unwrap_or_default();
    let out = tokio::task::spawn_blocking(move || {
        crate::collab::auth::logout(&token).map(|_| json!({"ok": true}))
    })
    .await;
    unwrap_api(out)
}

pub(super) async fn collab_me(State(state): State<CollabState>, headers: HeaderMap) -> Response {
    match auth_ctx(&state, &headers).await {
        Some(ctx) => Json(json!({"username": ctx.username, "role": ctx.role})).into_response(),
        None => (StatusCode::UNAUTHORIZED, Json(json!({"error":"未登录"}))).into_response(),
    }
}

/// 票据兑换(入伙的应用层部分;隧道层白名单在 P2 接入)。两条路,票据都记 used_by:
/// - **已登录**(带有效会话 token 且真实账号):不建新号,把本设备绑到当前账号 —— 保证
///   「同账号的人通过邀请码进来」;返回当前用户 + 原 token。
/// - 未登录:老路径,建账号+登记设备+签会话。
pub(super) async fn collab_redeem(
    State(state): State<CollabState>,
    headers: HeaderMap,
    Json(v): Json<Value>,
) -> Response {
    // 已登录路径:合成身份(user_id=0,全局口令)不算 —— 免鉴身份不允许绑设备入伙。
    let logged_in = match auth_ctx(&state, &headers).await {
        Some(ctx) if ctx.user_id > 0 => bearer_of(&headers),
        _ => None,
    };
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        if let Some(token) = logged_in {
            let user = crate::collab::auth::check_session(&token)?;
            crate::collab::identity::redeem_ticket_existing(
                &s_of(&v, "code"),
                &user,
                &s_of(&v, "deviceName"),
                &s_of(&v, "nodeId"),
            )?;
            return Ok(json!({"user": user, "token": token}));
        }
        // 成员主机上「未登录 + 邀请码 → 就地建号」这条老路会造出孤岛账号,堵掉:
        // 邀请码在联邦模式下只用于「已登录的云端账号把本设备绑进来」(上面那条路)。
        if let Some(url) = crate::collab::authority::upstream_url() {
            return Err(format!(
                "本主机的账号由云端账号中心({url})统一管理:请先用云端账号登录,再用邀请码入伙"
            ));
        }
        let (u, token) = crate::collab::identity::redeem_ticket(
            &s_of(&v, "code"),
            &s_of(&v, "username"),
            &s_of(&v, "password"),
            &s_of(&v, "displayName"),
            &s_of(&v, "deviceName"),
            &s_of(&v, "nodeId"),
        )?;
        Ok(json!({"user": u, "token": token}))
    })
    .await;
    unwrap_api(out)
}

pub(super) async fn collab_ticket(
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
    // 分享码 = 裸码 + 主机可达地址(启动时探测/环境覆写);成员端粘一串即入伙。
    let advertise = state.advertise.read().clone();
    let out = tokio::task::spawn_blocking(move || {
        let role = {
            let r = s_of(&v, "role");
            if r.is_empty() {
                "collaborator".into()
            } else {
                r
            }
        };
        let t = crate::collab::identity::create_ticket(&role, &s_of(&v, "note"))?;
        let share = crate::collab::identity::encode_share_code(&t.code, &advertise);
        ok(json!({ "code": t.code, "role": t.role, "expires_at": t.expires_at, "share": share }))
    })
    .await;
    unwrap_api(out)
}

pub(super) async fn collab_users(State(state): State<CollabState>, headers: HeaderMap) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    if role_rank(&ctx.role) < 3 {
        return forbid();
    }
    let out = tokio::task::spawn_blocking(|| crate::collab::auth::list_users().and_then(ok)).await;
    unwrap_api(out)
}

pub(super) async fn collab_user_disable(
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
        let id = v
            .get("userId")
            .and_then(|x| x.as_i64())
            .ok_or("缺 userId")?;
        let dis = v.get("disabled").and_then(|x| x.as_bool()).unwrap_or(true);
        crate::collab::auth::set_user_disabled(id, dis)?;
        // 停用即把该账号名下所有中继挂牌踢下线(关监听/断连/删挂牌行),不留「还能被反代到」的尾巴。
        #[cfg(feature = "collab-net")]
        if dis {
            crate::collab::gateway::unregister_user(id);
        }
        Ok(json!({"ok": true}))
    })
    .await;
    unwrap_api(out)
}

/// 成员主机上建号没有意义:账号是云端签发的,本机建出来的号在别处一律不认。
/// 权威机与本地权威(老部署)照旧放行。
pub(super) fn reject_if_delegated(what: &str) -> Option<Response> {
    let url = crate::collab::authority::upstream_url()?;
    Some(
        (
            StatusCode::FORBIDDEN,
            Json(json!({
                "error": format!("本机不是账号中心,{what}请到账号中心操作:{url}")
            })),
        )
            .into_response(),
    )
}

/// 联邦账号的密码与邮箱只有账号权威说得算:在成员主机上改,不但对别的主机毫无作用,
/// 设密码更是给它开了一条绕过权威的本地后门(那一位本该只能凭权威签的断言进门)。
/// 故按**账号**拦 —— 同一台机器上的本地应急账号不受影响,照旧能就地重置。
pub(super) fn ensure_not_federated(user_id: i64, what: &str) -> Result<(), String> {
    if !crate::collab::auth::is_federated_account(user_id)? {
        return Ok(());
    }
    let tail = crate::collab::authority::upstream_url()
        .map(|u| format!(":{u}"))
        .unwrap_or_default();
    Err(format!("这是云端账号,{what}请到账号中心操作{tail}"))
}

/// owner 直接重置任意用户密码(邮箱没绑/邮件服务没配时的兜底通道)。
/// 改密即踢掉该用户全部旧会话。
pub(super) async fn collab_admin_user_reset_password(
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
    let actor = ctx.username.clone();
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let id = v
            .get("userId")
            .and_then(|x| x.as_i64())
            .ok_or("缺 userId")?;
        ensure_not_federated(id, "改密码")?;
        crate::collab::auth::set_password(id, &s_of(&v, "newPassword"))?;
        crate::collab::db::audit(&actor, "user.reset_password", &id.to_string(), "owner 重置");
        Ok(json!({"ok": true}))
    })
    .await;
    unwrap_api(out)
}

// ── owner 远程账号管理 ────────────────────────────────────────────────────
//
// 此前云机上开一个账号只有两条路:开自助注册(等于把主机 RCE 交给公网,见 account_signup
// 的注释),或者 SSH 进服务器跑 `seed_authority_account`。前者不能开,后者要有服务器 shell,
// 于是「远程管好账号」这件事实际上没有出口。这组端点补的就是它:**owner 会话**从任何地方
// (桌面、手机壳、浏览器)都能建号/改号/删号,权限门槛与既有 admin/* 完全一致。
//
// 注意它们操作的始终是**本进程这台机器**的 users 表:在账号权威上是全局账号,
// 在成员主机上是本机成员资格。密码与邮箱只属于权威,故成员主机上被 reject_if_delegated 挡住。

/// owner 建号。权威机上顺带签发全局 uid(即「一个账号在所有主机上登」的那个身份);
/// 本地权威(老部署)则只建本机账号。邮箱可选,**不发验证码**——这是管理员当面开号,
/// 不是自助注册,邮箱由管理员负责填对(它只用于日后自助找回)。
pub(super) async fn collab_admin_account_create(
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
    if let Some(r) = reject_if_delegated("建账号") {
        return r;
    }
    let actor = ctx.username.clone();
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let username = s_of(&v, "username");
        let display = {
            let d = s_of(&v, "displayName");
            if d.trim().is_empty() {
                username.clone()
            } else {
                d
            }
        };
        let role = {
            let r = s_of(&v, "role");
            if r.trim().is_empty() {
                "collaborator".to_string()
            } else {
                r
            }
        };
        let (u, uid) = crate::collab::auth::create_account_full(
            &username,
            &s_of(&v, "password"),
            &display,
            &s_of(&v, "email"),
            &role,
            crate::collab::authority::is_authority(),
        )?;
        crate::collab::db::audit(&actor, "user.create", &u.role, "owner 远程建号");
        Ok(json!({"user": u, "uid": uid}))
    })
    .await;
    unwrap_api(out)
}

/// owner 改号:昵称 / 邮箱 / 角色,三项都可单独改(字段缺席 = 不动这一项)。
/// 云端账号的邮箱改不了(那把找回密码的钥匙在账号中心);角色是**本机**的,哪台机器都可改。
pub(super) async fn collab_admin_account_update(
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
    let actor = ctx.username.clone();
    let me = ctx.user_id;
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let id = i_of(&v, "userId").ok_or("缺 userId")?;
        let display = v.get("displayName").and_then(|x| x.as_str());
        let email = v.get("email").and_then(|x| x.as_str());
        if email.is_some() {
            ensure_not_federated(id, "改邮箱")?;
        }
        if display.is_some() || email.is_some() {
            crate::collab::auth::update_user_profile(id, display, email)?;
        }
        if let Some(role) = v.get("role").and_then(|x| x.as_str()) {
            // 自己把自己降级 = 当场把管理面关在门外(且不一定是最后一个 owner,
            // last-owner 那道闸拦不住),故单独拦一道。
            if id == me && role != "owner" {
                return Err("不能给自己降级 —— 请让另一个管理员来操作".into());
            }
            crate::collab::auth::set_user_role(id, role)?;
        }
        crate::collab::db::audit(&actor, "user.update", &id.to_string(), "owner 远程改号");
        Ok(json!({"ok": true}))
    })
    .await;
    unwrap_api(out)
}

/// owner 删号。级联清掉该账号的会话、设备、项目/团队成员关系(外键 ON DELETE CASCADE)。
/// 不能删自己,也不能删最后一个在岗 owner。
pub(super) async fn collab_admin_account_delete(
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
    let actor = ctx.username.clone();
    let me = ctx.user_id;
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let id = i_of(&v, "userId").ok_or("缺 userId")?;
        if id == me {
            return Err("不能删除自己的账号".into());
        }
        // 先把该账号名下的中继挂牌摘掉,再删行 —— 顺序反了会留下「查不到主人的挂牌」。
        #[cfg(feature = "collab-net")]
        crate::collab::gateway::unregister_user(id);
        let username = crate::collab::auth::delete_user(id)?;
        crate::collab::db::audit(&actor, "user.delete", &username, "owner 远程删号");
        Ok(json!({"ok": true, "username": username}))
    })
    .await;
    unwrap_api(out)
}

/// 给 v2.5.0 之前建的老账号补签全局 uid(仅账号权威)。没有 uid 的账号登不了别的主机,
/// `authority_login` 会直接拒绝并叫人来找管理员 —— 这就是那个补签口。已有 uid 的原样返回。
pub(super) async fn collab_admin_account_uid_backfill(
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
    if !crate::collab::authority::is_authority() {
        return not_authority();
    }
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let id = i_of(&v, "userId").ok_or("缺 userId")?;
        Ok(json!({"uid": crate::collab::auth::backfill_uid(id)?}))
    })
    .await;
    unwrap_api(out)
}

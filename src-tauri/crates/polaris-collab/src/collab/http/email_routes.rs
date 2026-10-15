use super::*;

// ── GitHub 式注册与团队 ──

/// 注册:**默认邀请/票据制**(安全默认)。显式 POLARIS_OPEN_SIGNUP=1 才开放自助注册。
/// 为什么默认关:开放注册 = 陌生人可自助入伙,而入伙后能建项目、push 仓库,checks 会在主机上
/// 跑该仓库的构建(npm run build 直接执行 package.json 脚本;cargo check 会运行 build.rs 与
/// proc-macro)—— 本质是「在主机上执行不可信代码」= 主机 RCE。故收紧为邀请制,把能 push 代码的
/// 人限定在管理员亲自发票据请进来的可信成员。
pub(super) async fn collab_signup(Json(v): Json<Value>) -> Response {
    if let Some(r) = refuse_local_account_creation() {
        return r;
    }
    let open = std::env::var("POLARIS_OPEN_SIGNUP")
        .map(|s| s == "1" || s.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if !open {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error":"本主机为邀请制,请找管理员要邀请票据"})),
        )
            .into_response();
    }
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let u = crate::collab::auth::create_user(
            &s_of(&v, "username"),
            &s_of(&v, "password"),
            "collaborator",
            &s_of(&v, "displayName"),
        )?;
        let (_, token) =
            crate::collab::auth::login(&u.username, &s_of(&v, "password"), &s_of(&v, "deviceId"))?;
        Ok(json!({"user": u, "token": token}))
    })
    .await;
    unwrap_api(out)
}

/// 成员主机(delegated)上不许就地建号 —— 账号只能在权威那儿注册,否则又长出一座孤岛,
/// 「一个账号到处登」当场破功。返回 Some 即调用方应直接把它当响应返回。
pub(super) fn refuse_local_account_creation() -> Option<Response> {
    let url = crate::collab::authority::upstream_url()?;
    Some(
        (
            StatusCode::FORBIDDEN,
            Json(json!({
                "error": "本主机的账号由云端账号中心统一管理,请到账号中心注册后再登录",
                "authorityUrl": url,
            })),
        )
            .into_response(),
    )
}

// ── 邮箱注册 / 找回密码(验证码 = 邮箱所有权证明) ──

/// 邮箱服务状态(公开,无鉴权):前端据此决定「注册」入口亮不亮、「忘记密码」走不走邮箱。
pub(super) async fn collab_email_status() -> Response {
    // configured/signup_open 都要读 collab.db(meta 表),放阻塞池。
    let out = tokio::task::spawn_blocking(|| -> Result<Value, String> {
        Ok(json!({
            "configured": crate::collab::mail::configured(),
            "signupOpen": crate::collab::mail::signup_open(),
        }))
    })
    .await;
    unwrap_api(out)
}

/// 发验证码。purpose=signup:邮箱须未被绑定;purpose=reset:邮箱须已绑定账号。
/// 频控在 mail::issue_code(60s 重发间隔 + 每小时 5 封)。
pub(super) async fn collab_email_send_code(Json(v): Json<Value>) -> Response {
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let email = s_of(&v, "email");
        let purpose = s_of(&v, "purpose");
        match purpose.as_str() {
            "signup" => {
                // 权威机就是注册入口:只要邮件服务配齐就放行,不再受 email_signup 开关约束
                // (那个开关的语义是「本主机要不要收陌生人」,对权威机没有意义)。
                let ok = if crate::collab::authority::is_authority() {
                    crate::collab::mail::configured()
                } else {
                    crate::collab::mail::signup_open()
                };
                if !ok {
                    return Err("本主机未开放邮箱注册,请联系管理员或用邀请票据入伙".into());
                }
                if crate::collab::auth::find_user_by_email(&email)?.is_some() {
                    return Err("该邮箱已注册过账号,请直接登录或走「忘记密码」".into());
                }
            }
            "reset" => {
                if !crate::collab::mail::configured() {
                    return Err("邮箱服务未配置,无法自助找回,请联系管理员重置密码".into());
                }
                if crate::collab::auth::find_user_by_email(&email)?.is_none() {
                    return Err("该邮箱没有绑定任何账号".into());
                }
            }
            // 验证码登录:不需要密码,能收到这封信就证明是本人。邮箱须已绑定账号 ——
            // 与 reset 同一口径(都会告知「该邮箱没绑账号」,不额外多泄漏什么)。
            "login" => {
                if !crate::collab::mail::configured() {
                    return Err("邮箱服务未配置,无法用验证码登录,请用密码登录".into());
                }
                if crate::collab::auth::find_user_by_email(&email)?.is_none() {
                    return Err("该邮箱没有绑定任何账号".into());
                }
            }
            _ => return Err("purpose 须为 signup / reset / login".into()),
        }
        crate::collab::mail::issue_code(&email, &purpose)?;
        Ok(json!({"ok": true}))
    })
    .await;
    unwrap_api(out)
}

/// 邮箱验证码注册:验证通过即建号(collaborator)+ 绑邮箱 + 签会话,一步登录。
pub(super) async fn collab_email_signup(Json(v): Json<Value>) -> Response {
    if let Some(r) = refuse_local_account_creation() {
        return r;
    }
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        if !crate::collab::mail::signup_open() {
            return Err("本主机未开放邮箱注册,请联系管理员或用邀请票据入伙".into());
        }
        let email = s_of(&v, "email");
        crate::collab::mail::verify_code(&email, "signup", &s_of(&v, "code"))?;
        let username = s_of(&v, "username");
        let display = {
            let d = s_of(&v, "displayName");
            if d.trim().is_empty() {
                username.clone()
            } else {
                d
            }
        };
        let u = crate::collab::auth::create_user_with_email(
            &username,
            &s_of(&v, "password"),
            "collaborator",
            &display,
            &email,
        )?;
        let (_, token) =
            crate::collab::auth::login(&u.username, &s_of(&v, "password"), &s_of(&v, "deviceId"))?;
        Ok(json!({"user": u, "token": token}))
    })
    .await;
    unwrap_api(out)
}

/// 邮箱找回密码:验证码通过即换口令,该账号旧会话全部作废。回 username 提示用哪个名登录。
pub(super) async fn collab_email_reset(Json(v): Json<Value>) -> Response {
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let email = s_of(&v, "email");
        crate::collab::mail::verify_code(&email, "reset", &s_of(&v, "code"))?;
        let u = crate::collab::auth::find_user_by_email(&email)?.ok_or("该邮箱没有绑定任何账号")?;
        // 联邦账号的密码只有权威说了算:在成员主机上给它写一个本地密码,等于绕开权威
        // 开了一条本机后门(云端停用了这个人,他仍能凭这个本地密码在断网回落时进来)。
        // 按**账号**判,故同机的本地应急账号照旧能自助找回。
        ensure_not_federated(u.id, "改密码")?;
        crate::collab::auth::set_password(u.id, &s_of(&v, "newPassword"))?;
        Ok(json!({"ok": true, "username": u.username}))
    })
    .await;
    unwrap_api(out)
}

/// 邮箱验证码登录(本机会话):验证码 = 邮箱所有权证明,通过即签本机会话,不需要密码。
///
/// 成员主机上不开这条路 —— 那里的联邦账号身份归账号中心管,本机凭一份可能过期的
/// 邮箱副本放人进来,会绕过权威的停用/改密。本地账号模式与账号权威自己照常可用。
pub(super) async fn collab_email_login(Json(v): Json<Value>) -> Response {
    if let Some(r) = reject_if_delegated("验证码登录") {
        return r;
    }
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let email = s_of(&v, "email");
        crate::collab::mail::verify_code(&email, "login", &s_of(&v, "code"))?;
        let (u, token) =
            crate::collab::auth::login_with_verified_email(&email, &s_of(&v, "deviceId"))?;
        Ok(json!({"user": u, "token": token}))
    })
    .await;
    unwrap_api(out)
}

/// 自助注册是否开放。默认**关**:账号中心的 users 表同时是这台机器自己的成员表,
/// 放开等于把本机的 collaborator 会话发给任何人(成员能触发构建脚本 = 主机 RCE)。
/// 官方云机要开它,须同时开 `POLARIS_AUTHORITY_ONLY=1` 把「能跑东西」的那一面关掉。
pub(super) fn open_signup() -> bool {
    std::env::var("POLARIS_ACCOUNT_OPEN_SIGNUP")
        .map(|s| s == "1" || s.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// owner 读邮箱服务配置。SMTP 授权码永不回显,只回 passSet(是否已配置)。
pub(super) async fn collab_admin_email_config_get(
    State(state): State<CollabState>,
    headers: HeaderMap,
) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    if role_rank(&ctx.role) < 3 {
        return forbid();
    }
    if let Some(url) = crate::collab::authority::upstream_url() {
        return Json(json!({
            "managedHere": false,
            "authorityUrl": url,
            "configured": false,
            "host": "",
            "port": 465,
            "user": "",
            "from": "",
            "passSet": false,
            "signupOpen": false,
        }))
        .into_response();
    }
    let out = tokio::task::spawn_blocking(|| -> Result<Value, String> {
        let cfg = crate::collab::mail::config();
        let (host, port, user, from) = match &cfg {
            Some(c) => (c.host.clone(), c.port, c.user.clone(), c.from.clone()),
            None => (
                crate::collab::db::meta_get("smtp_host").unwrap_or_else(|| "smtp.qq.com".into()),
                crate::collab::db::meta_get("smtp_port")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(465),
                crate::collab::db::meta_get("smtp_user").unwrap_or_default(),
                crate::collab::db::meta_get("smtp_from").unwrap_or_default(),
            ),
        };
        Ok(json!({
            "managedHere": true,
            "authorityUrl": "",
            "configured": cfg.is_some(),
            "host": host,
            "port": port,
            "user": user,
            "from": from,
            "passSet": cfg.is_some(),
            "signupOpen": crate::collab::mail::signup_open(),
        }))
    })
    .await;
    unwrap_api(out)
}

/// owner 写邮箱服务配置(pass 留空 = 保留旧授权码);testTo 非空则顺手发一封测试信验通。
pub(super) async fn collab_admin_email_config_set(
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
    if let Some(r) = reject_if_delegated("配置验证码邮件") {
        return r;
    }
    let actor = ctx.username.clone();
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let user = s_of(&v, "user");
        if user.trim().is_empty() {
            return Err("请填发信邮箱(如 1799820934@qq.com)".into());
        }
        let host = {
            let h = s_of(&v, "host");
            if h.trim().is_empty() {
                "smtp.qq.com".into()
            } else {
                h
            }
        };
        let port = i_of(&v, "port").unwrap_or(465).clamp(1, 65535) as u16;
        let from = {
            let f = s_of(&v, "from");
            if f.trim().is_empty() {
                user.clone()
            } else {
                f
            }
        };
        let signup = v
            .get("signupOpen")
            .and_then(|x| x.as_bool())
            .unwrap_or(true);
        crate::collab::mail::save_config(&host, port, &user, &s_of(&v, "pass"), &from, signup)?;
        crate::collab::db::audit(&actor, "email.config", &user, "SMTP 配置更新");
        let test_to = s_of(&v, "testTo");
        if !test_to.trim().is_empty() {
            crate::collab::mail::send_mail(
                test_to.trim(),
                "【Polaris】邮箱服务测试",
                "看到这封信,说明 Polaris 的 SMTP 邮箱服务已配置成功。",
            )?;
        }
        Ok(json!({"ok": true, "configured": crate::collab::mail::configured()}))
    })
    .await;
    unwrap_api(out)
}

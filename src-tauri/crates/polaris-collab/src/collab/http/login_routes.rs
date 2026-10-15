use super::*;

// ───────────────────────── /api/collab 账号与会话 ─────────────────────────
//
// 登录发生在隧道之内(v8 第八节):这些端点本身不再叠全局口令,密码与会话即凭据。
// bootstrap 仅在零账号时可用;admin/* 全部要求 owner 会话。

/// 首启建 owner(仅零账号时放行,防抢注)。
pub(super) async fn collab_bootstrap(
    State(state): State<CollabState>,
    open: Option<axum::Extension<OpenGate>>,
    headers: HeaderMap,
    Json(v): Json<Value>,
) -> Response {
    // loopback 不是身份边界：恶意网页可通过跨源请求扫描本机端口，本地普通进程也可直连。
    // 设了机器口令就必须出示它；桌面内嵌主机会生成随机口令并仅经 Tauri IPC
    // 交给自己的前端，绝不通过 HTTP 状态接口泄露。
    let supplied = bearer_of(&headers);
    let setup_ok = state
        .auth_token
        .as_ref()
        .as_ref()
        .map(|expected| supplied.as_deref() == Some(expected.as_str()))
        .unwrap_or(false);
    // ★ 免口令模式(2026-07-25 起是 Docker/NAS 的默认)：机器压根没有口令可出示，
    //   只认上面那条的话第一个账号永远建不出来 —— 协作/团队/联邦整条线锁死。
    //   这里认外壳算好的来源闸(与 /api/invoke 同一套判定，含 POLARIS_LAN_ONLY)。
    //   放开的边界很窄：bootstrap 本身在 create_initial_owner 里以 IMMEDIATE 事务
    //   原子限定「库里零账号才可建」，且这台机器的应用面早已把任何够格来源当 owner。
    let open_ok = open.map(|e| e.0 .0).unwrap_or(false);
    if !setup_ok && !open_ok {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"首次初始化需要服务器访问口令"})),
        )
            .into_response();
    }
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let u = crate::collab::auth::create_initial_owner(
            &s_of(&v, "username"),
            &s_of(&v, "password"),
            &s_of(&v, "displayName"),
        )?;
        let (_, token) =
            crate::collab::auth::login(&u.username, &s_of(&v, "password"), &s_of(&v, "deviceId"))?;
        // 「把这台电脑设为主机」流程:主机自己的前端在 bootstrap 时自报 hostSelf,
        // 顺手把本机登记进设备白名单并落 meta,设备页据此点亮「主机」徽标。
        // (NAS/远程 bootstrap 不带 hostSelf → 行为与旧版完全一致。)
        if v.get("hostSelf").and_then(|x| x.as_bool()).unwrap_or(false) {
            let node = s_of(&v, "deviceId");
            if !node.is_empty() {
                let name = std::env::var("COMPUTERNAME")
                    .or_else(|_| std::env::var("HOSTNAME"))
                    .map(|h| format!("{h}(主机)"))
                    .unwrap_or_else(|_| "主机".into());
                let _ = crate::collab::identity::add_device(u.id, &name, &node);
                let _ = crate::collab::db::meta_set("host_node_id", &node);
            }
        }
        Ok(json!({"user": u, "token": token}))
    })
    .await;
    unwrap_api(out)
}

/// 密码登录。成员主机(delegated)上密码**不在本机**,故转交账号权威代验:
/// 主机 → 权威取断言 → 本地验签 → 签本机会话。这条路是给「客户端只连得上主机、
/// 连不上云机」的场景兜底(局域网里的手机);客户端自己够得着云机时应当直接走
/// `/api/account/login` + `/api/collab/login_assertion`,不占主机的出口带宽。
///
/// 权威不可达时**回落本地密码库**:应急 owner、老的本地账号仍然进得去 ——
/// 否则云机一挂,全世界的桌面主机一起锁死,那是把单点故障从「登录慢」升级成「全线瘫痪」。
pub(super) async fn collab_login(Json(v): Json<Value>) -> Response {
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let username = s_of(&v, "username");
        let password = s_of(&v, "password");
        let device_id = s_of(&v, "deviceId");
        if crate::collab::authority::upstream_url().is_some() {
            match crate::collab::authority::upstream_login(&username, &password) {
                Ok(assertion) => {
                    let (u, token) =
                        crate::collab::authority::login_with_assertion(&assertion, &device_id)?;
                    return Ok(json!({"user": u, "token": token, "via": "authority"}));
                }
                Err(e) => {
                    // 只有「连不上」才回落本地;权威明确说密码错就得原样报错,
                    // 否则等于给联邦账号开了一条「云端拒了还能试本机」的旁门。
                    let unreachable = e.contains("连不上") || e.contains("未配置账号权威");
                    if !unreachable {
                        return Err(e);
                    }
                    // 回落本地。本地也进不去时**不能**报「用户名或密码错误」—— 真正的原因是
                    // 云端账号中心连不上,报成密码错会让人对着正确的密码反复重试查不出所以然。
                    match crate::collab::auth::login(&username, &password, &device_id) {
                        Ok((u, token)) => {
                            return Ok(json!({"user": u, "token": token, "via": "local-fallback"}))
                        }
                        Err(local_err) => {
                            return Err(format!(
                                "连不上云端账号中心,且本机没有这个账号的本地凭据 —— \
                                 你的账号在云端,请等网络恢复再登(本机回落结果:{local_err})"
                            ))
                        }
                    }
                }
            }
        }
        let (u, token) = crate::collab::auth::login(&username, &password, &device_id)?;
        Ok(json!({"user": u, "token": token}))
    })
    .await;
    unwrap_api(out)
}

// ───────────────────────── 账号权威 /api/account/* ─────────────────────────
//
// 权威机(云机,POLARIS_ACCOUNT_AUTHORITY=1)是**唯一**能建账号、验密码的地方。
// 它签一张短命的身份断言(5 分钟),客户端拿着它去任何一台成员主机换本机会话。
// 成员主机纯本地验签、不联网 —— 家里的桌面断网了照样能让人登进来。

pub(super) fn not_authority() -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({"error":"本机不是账号权威(未设 POLARIS_ACCOUNT_AUTHORITY=1)"})),
    )
        .into_response()
}

/// 发一枚**登录**验证码。整套账号体系唯一的公开发码口。
///
/// 刻意不回答「这个邮箱有没有注册过」——不管有没有,响应都一样。否则这个端点就成了
/// 一台免费的「查某人是不是 Polaris 用户」的机器。首登自动建号也让这个区分本就没意义了。
///
/// 频控见 [`crate::collab::mail`]:按邮箱 + 按来源 IP 两道,都**落库**(重启不清零)。
pub(super) async fn account_send_code(
    peer: Option<axum::extract::ConnectInfo<std::net::SocketAddr>>,
    headers: HeaderMap, Json(v): Json<Value>,
) -> Response {
    let ip = match client_ip(&headers, peer) {
        Ok(ip) => ip,
        Err(error) => return err_resp(error),
    };
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        // 成员主机:替客户端跑一趟公网。手机常常只连得上家里这台机器、连不上云机 ——
        // 没有这条转发,「用邮箱登录」在那种网络下直接不可用,只能退回粘连接码。
        if !crate::collab::authority::is_authority() {
            return crate::collab::authority::upstream_forward("/api/account/send_code", &v);
        }
        if !crate::collab::mail::configured() {
            return Err("本账号中心还没配邮件服务,发不出验证码 —— 请联系管理员".into());
        }
        crate::collab::mail::issue_code_from(&s_of(&v, "email"), "login", &ip)?;
        Ok(json!({ "sent": true, "ttl": 600 }))
    })
    .await;
    unwrap_api(out)
}

/// 全局改密(邮箱验证码):改的是权威库里的密码,**所有**主机随之生效 ——
/// 因为没有任何一台主机存着这个密码。
pub(super) async fn account_reset(Json(v): Json<Value>) -> Response {
    if !crate::collab::authority::is_authority() {
        return not_authority();
    }
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let email = s_of(&v, "email");
        crate::collab::mail::verify_code(&email, "reset", &s_of(&v, "code"))?;
        let u = crate::collab::auth::find_user_by_email(&email)?.ok_or(crate::collab::mail::INVALID_CODE)?;
        crate::collab::auth::set_password(u.id, &s_of(&v, "newPassword"))?;
        Ok(json!({"ok": true, "username": u.username}))
    })
    .await;
    unwrap_api(out)
}

/// 成员主机进门:拿权威签的断言换本机会话 token。
///
/// 这里**不联网** —— 只做本地验签 + 落本机成员资格。故家里的桌面主机断网时,
/// 只要手机自己连得上云机把断言取回来,照样登得进桌面。
pub(super) async fn collab_login_assertion(Json(v): Json<Value>) -> Response {
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let (u, token) = crate::collab::authority::login_with_assertion(
            &s_of(&v, "assertion"),
            &s_of(&v, "deviceId"),
        )?;
        Ok(json!({"user": u, "token": token}))
    })
    .await;
    unwrap_api(out)
}

/// 云端账号 + 邀请码 → 成为本机成员。
///
/// 联邦模式下,「你是谁」由云端证明,「这台主机欢不欢迎你」由本机的邀请票据证明,
/// 两把钥匙缺一不可 —— 只有前者就成了「谁在云端注册都能登进你家电脑」。
pub(super) async fn collab_join(Json(v): Json<Value>) -> Response {
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let (u, token) = crate::collab::authority::join_with_ticket(
            &s_of(&v, "assertion"),
            &s_of(&v, "code"),
            &s_of(&v, "deviceName"),
            &s_of(&v, "nodeId"),
        )?;
        Ok(json!({"user": u, "token": token}))
    })
    .await;
    unwrap_api(out)
}

//! Account discovery and login responses (including authority relay URLs).
use super::*;

/// 账号体系自述(公开):告诉客户端「本机的账号该在哪儿登」。
/// 三种回答对应三种部署:authority(我就是权威)/ delegated(去这个 URL 登)/ local(老行为,就地登)。
pub(super) async fn account_info() -> Response {
    let out = tokio::task::spawn_blocking(|| -> Result<Value, String> {
        use crate::collab::authority::{self, Mode};
        crate::collab::interconnect::load()?;
        // emailRequired 恒 false:邮箱是**可选**的(绑了才有自助找回)。
        // emailCapable 告诉前端「这儿发得出验证码吗」—— 发不出就别让人填邮箱空等。
        Ok(match authority::mode() {
            Mode::Authority => json!({
                "mode": "authority",
                "kid": authority::kid().unwrap_or_default(),
                "emailRequired": false,
                "emailCapable": crate::collab::mail::configured(),
                // 前端据此决定「注册」入口亮不亮:关着还亮就是骗用户填一遍再报 403。
                "signupOpen": std::env::var("POLARIS_ACCOUNT_OPEN_SIGNUP")
                    .map(|s| s == "1" || s.eq_ignore_ascii_case("true"))
                    .unwrap_or(false),
            }),
            Mode::Delegated(url) => {
                let pin = authority::pinned();
                json!({
                    "mode": "delegated",
                    "authorityUrl": url,
                    "trusted": pin.is_some(),
                    "kid": pin.map(|(_, _, k)| k).unwrap_or_default(),
                    "emailRequired": false,
                })
            }
            Mode::Local => json!({"mode": "local", "emailRequired": false}),
        })
    })
    .await;
    unwrap_api(out)
}

/// 权威公钥(公开):成员主机首次接触时钉住这一串。
pub(super) async fn account_pubkey() -> Response {
    if !crate::collab::authority::is_authority() {
        return not_authority();
    }
    let out = tokio::task::spawn_blocking(|| -> Result<Value, String> {
        let pk = crate::collab::authority::public_key_b64()?;
        Ok(json!({"fingerprint": crate::collab::interconnect::fingerprint(&pk)?,
            "publicKey": pk, "kid": crate::collab::authority::kid_of(&pk), "alg": "ed25519"}))
    })
    .await;
    unwrap_api(out)
}

/// 全局注册。邮箱**可选**:
///  · 不填邮箱 → 直接建号(没有自助找回,忘了密码得找管理员重置)。
///  · 填了邮箱 → 必须同时给出发到该邮箱的验证码,验过才绑 —— 绑一个没验证过的邮箱
///    等于把「找回密码」的钥匙交给一个不确定属于谁的地址,比不绑还危险。
///
/// **默认关闭自助注册**(须显式 `POLARIS_ACCOUNT_OPEN_SIGNUP=1` 才开)。
/// 原因:账号中心的 `users` 表同时就是这台机器自己的成员表 —— 放开自助注册,
/// 等于任何人都能拿到本机的 collaborator 会话,而成员能建项目、触发 checks 跑构建脚本,
/// 本质是主机 RCE。这与 `collab_signup` 默认邀请制是同一条理由,不能因为「它是账号中心」就松口。
/// 关闭时建号走服务端的 `seed_authority_account` 工具(要能读写权威库,不是远程口子)。
pub(super) async fn account_signup(Json(v): Json<Value>) -> Response {
    if !crate::collab::authority::is_authority() {
        return not_authority();
    }
    if !open_signup() {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error":"本账号中心未开放自助注册,请联系管理员开通账号"})),
        )
            .into_response();
    }
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let email = s_of(&v, "email");
        if !email.trim().is_empty() {
            if !crate::collab::mail::configured() {
                return Err("本账号中心没配邮件服务,发不出验证码 —— 请留空邮箱直接注册".into());
            }
            crate::collab::mail::verify_code(&email, "signup", &s_of(&v, "code"))?;
        }
        let username = s_of(&v, "username");
        let display = {
            let d = s_of(&v, "displayName");
            if d.trim().is_empty() {
                username.clone()
            } else {
                d
            }
        };
        let (u, uid) = crate::collab::auth::create_authority_account(
            &username,
            &s_of(&v, "password"),
            &display,
            &email,
        )?;
        let assertion =
            crate::collab::authority::sign_assertion(&uid, &u.username, &email, &display)?;
        Ok(json!({"user": u, "uid": uid, "email": email, "assertion": assertion,
            "relays": crate::collab::interconnect::advertised_relays()?}))
    })
    .await;
    unwrap_api(out)
}

/// 全局登录:用户名**或邮箱** + 密码 → 身份断言。权威只回答「你是谁」,
/// 不回答「你在某台主机上能干什么」—— 那是各主机自己的账。
pub(super) async fn account_login(Json(v): Json<Value>) -> Response {
    if !crate::collab::authority::is_authority() {
        return not_authority();
    }
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let ident = {
            let u = s_of(&v, "username");
            if u.trim().is_empty() {
                s_of(&v, "email")
            } else {
                u
            }
        };
        let (u, uid, email) = crate::collab::auth::authority_login(&ident, &s_of(&v, "password"))?;
        let assertion =
            crate::collab::authority::sign_assertion(&uid, &u.username, &email, &u.display_name)?;
        Ok(json!({
            "user": {"username": u.username, "displayName": u.display_name, "email": email},
            "uid": uid,
            "assertion": assertion,
            "relays": crate::collab::interconnect::advertised_relays()?,
        }))
    })
    .await;
    unwrap_api(out)
}

/// 全局验证码登录(账号权威):邮箱 + 验证码 → 身份断言,拿着它能进任意一台成员主机。
/// 与 `/api/account/login` 的区别只在于「凭什么证明是你」——那个用密码,这个用邮箱验证码。
///
/// **首登即注册**:这个邮箱还没有账号时当场建一个(受 `POLARIS_ACCOUNT_OPEN_SIGNUP` 闸控)。
/// 注册页因此可以整个消失 —— 用户从没填过用户名、没设过密码,只证明了「这个邮箱是我的」。
/// 建出来的账号密码位是哨兵串,永远不可能凭密码进来。
pub(super) async fn account_login_code(Json(v): Json<Value>) -> Response {
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        // 与 send_code 同一口径:成员主机当管子,把验证码原样递给账号中心、把断言原样带回来。
        // 主机自己不看、不改、不留 —— 它本来就没有能力验这枚码。
        if !crate::collab::authority::is_authority() {
            return crate::collab::authority::upstream_forward("/api/account/login_code", &v);
        }
        let email = s_of(&v, "email");
        // 验证码必须**先**过:否则「首登自动建号」就成了任何人往任意邮箱建号的口子。
        crate::collab::mail::verify_code(&email, "login", &s_of(&v, "code"))?;
        let (u, uid, email) = match crate::collab::auth::authority_identity_by_email(&email) {
            Ok(v) => v,
            Err(e) => {
                // 只有「没这个账号」才走自动建号;停用、缺 uid 这些要原样报出去,
                // 否则会绕过管理员的停用决定,凭空再造一个能用的号。
                if !e.contains("没有绑定任何账号") {
                    return Err(e);
                }
                if !open_signup() {
                    return Err("本账号中心未开放自助注册,请联系管理员开通账号".into());
                }
                match crate::collab::auth::create_account_by_verified_email(&email) {
                    Ok((u, uid)) => {
                        let e2 = crate::collab::auth::validate_email(&email)?;
                        (u, uid, e2)
                    }
                    // 并发首登:两台设备同时拿着同一枚码进来,一个建成、一个撞唯一索引。
                    // 撞了的那个直接读既有账号即可 —— 用户看到的是两台都登进去了。
                    Err(_) => crate::collab::auth::authority_identity_by_email(&email)?,
                }
            }
        };
        let assertion =
            crate::collab::authority::sign_assertion(&uid, &u.username, &email, &u.display_name)?;
        Ok(json!({
            "user": {"username": u.username, "displayName": u.display_name, "email": email},
            "uid": uid,
            "assertion": assertion,
            "relays": crate::collab::interconnect::advertised_relays()?,
        }))
    })
    .await;
    unwrap_api(out)
}


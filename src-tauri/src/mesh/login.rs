//! Login and durable enrollment adapters.
use super::*;

/// 发一枚登录验证码到邮箱。整套登录流程的第一步,也是用户唯一要动手的两步之一。
///
/// `url` 留空 = 官方云端账号中心。这个参数只为自建账号中心的人保留,UI 收在「高级」里。
#[cfg_attr(feature = "desktop", tauri::command)]
#[allow(non_snake_case)]
pub async fn account_send_code(
    email: String,
    url: Option<String>,
    allowInsecureHttp: Option<bool>,
) -> Result<Value, String> {
    let url = validate_authority_url(&authority_of(url)?, allowInsecureHttp.unwrap_or(false))?;
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        crate::collab::authority::pin_explicit(&url)?;
        agent()?
            .post(&format!("{url}/api/account/send_code"))
            .send_json(json!({ "email": email.trim() }))
            .map_err(cloud_err)?;
        Ok(json!({ "sent": true }))
    })
    .await
    .map_err(|e| format!("发码任务失败:{e}"))??;
    Ok(out)
}

/// **一步登录**:邮箱 + 验证码 → 从此这台机器就在你的设备网里。
///
/// 用户只做两件事(填邮箱、填码),下面这四件事全在这一个命令里做完 ——
/// 此前它们分散在「注册页 / 登录页 / 设为主机 / 入网表单」四个地方,那正是「怎么还要登两个账号」
/// 的来源:
///   ① 拿验证码换身份断言(账号不存在就当场建号 —— 注册页因此可以整个消失);
///   ② 把这个账号中心**钉在本机**(少了它,别的设备连过来会被自己挡回去);
///   ③ 拿断言进**本机**的门,确保当前账号确实获准使用这台主机;
///   ④ 前三步都通过后才入设备网,换一把长期设备密钥并与 UID/URL 原子落库。
///
/// `inviteCode` 只在第 ③ 步真的被本机挡住时才用得上(这台机器已经有**别人**当主人了)。
/// 裸码和 `PLRS1-…` 分享码都收 —— 归一在 `authority::normalize_invite`。
///
/// 云端 enroll 或最终落库失败会直接保留在登录界面并允许重试；不会再保存一份缺 UID、
/// authority pin 或本机成员资格的“已入网”半成品。已经安全建立的本机 pin/成员关系可幂等复用。
#[cfg_attr(feature = "desktop", tauri::command)]
#[allow(non_snake_case)]
pub async fn account_login_code(
    email: String,
    code: String,
    url: Option<String>,
    inviteCode: Option<String>,
    allowInsecureHttp: Option<bool>,
) -> Result<Value, String> {
    let url = validate_authority_url(&authority_of(url)?, allowInsecureHttp.unwrap_or(false))?;
    // NodeId 由本机密钥推导,不需要隧道在跑 —— 老版本那句「请先设为主机」其实是误导:
    // 真正需要主机在跑的是「别人连得进来」,不是「我拿得到身份」。所以这里不再拦人,
    // 拿不到才报错(那是密钥文件写不进去,是真问题)。
    let node_id = my_node_id();
    if node_id.is_empty() {
        return Err("本机 P2P 身份生成失败(数据目录写不进去?)—— 换个数据目录再试".into());
    }
    let invite = inviteCode.unwrap_or_default();
    let out = tokio::task::spawn_blocking({
        let url = url.clone();
        move || -> Result<Value, String> {
            crate::collab::authority::pin_explicit(&url)?;
            // ① 验证码 → 身份断言(账号不存在则当场建号,由账号中心决定)。
            let resp = agent()?
                .post(&format!("{url}/api/account/login_code"))
                .send_json(json!({ "email": email.trim(), "code": code.trim() }))
                .map_err(cloud_err)?;
            let v: Value = resp
                .into_json()
                .map_err(|e| format!("账号中心返回的不是 JSON:{e}"))?;
            crate::collab::interconnect::apply_received_relays(&v)?;
            let assertion = v
                .get("assertion")
                .and_then(|x| x.as_str())
                .ok_or("账号中心没返回身份断言 —— 确认这个地址开了 POLARIS_ACCOUNT_AUTHORITY=1")?
                .to_string();
            let email_back = v
                .get("user")
                .and_then(|u| u.get("email"))
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            Ok(json!({"assertion": assertion, "email": email_back}))
        }
    })
    .await
    .map_err(|e| format!("登录任务失败:{e}"))??;

    let assertion = out["assertion"].as_str().ok_or("登录响应缺断言")?.to_string();
    let email = out["email"].clone();
    let mut result = orchestration::complete_login(url, node_id, assertion, invite, true).await?;
    result["email"] = email;
    Ok(result)
}

/// 已有一张断言时的入网 + 进本机门。`account_login_code` 与 `mesh_join` 共用这一段,
/// 免得两条路各写一份、慢慢漂成两个语义。**阻塞**,调用方须放 spawn_blocking。
///
/// `console`=用户就坐在这台机器前亲手登的(桌面 App)。它决定第 ④ 步走哪一档准入:
/// 控制台可以认领这台机器上原有的那个账号(见 `Admission::LocalConsole`),远程递进来的
/// 断言不行。`invite` 是最后的兜底:这台机器已经有**别人**当主人时才用得上。
pub(super) fn enroll_with_assertion(
    url: &str,
    node_id: &str,
    assertion: &str,
    invite: &str,
    console: bool,
) -> Result<Value, String> {
    // ② 先钉账号中心。若本机已经信任另一套 authority，必须在取得长期 meshKey 之前
    // fail closed；否则会落出“云端已入网、别的设备永远进不来”的半成品。
    crate::collab::authority::pin_explicit(url)
        .map_err(|e| format!("账号中心信任未建立，尚未入网：{e}"))?;

    // ③ 再让当前账号进本机的门。邀请码兜底也必须在 enroll 前完成；本地准入失败时
    // 用户仍停留在登录界面，可以补设备码重试，而不是拿到 meshKey 后被假成功页藏住。
    let first = if console {
        crate::collab::authority::login_with_assertion_local(assertion, node_id)
    } else {
        crate::collab::authority::login_with_assertion(assertion, node_id)
    };
    let (user, token) = match first {
        Ok(v) => v,
        Err(e) if crate::collab::authority::normalize_invite(invite).is_empty() => {
            return Err(format!("本机成员准入未完成，尚未入网：{e}"));
        }
        Err(_) => {
            crate::collab::authority::join_with_ticket(assertion, invite, &my_name(), node_id)
                .map_err(|e| format!("本机成员准入未完成，尚未入网：{e}"))?
        }
    };

    // ④ 本地两道前置闸均通过后，才去云端换长期设备密钥。
    let resp = agent()?
        .post(&format!("{url}/api/mesh/enroll"))
        .send_json(json!({
            "assertion": assertion,
            "nodeId": node_id,
            "name": my_name(),
            "os": my_os(),
            "ver": my_ver(),
        }))
        .map_err(cloud_err)?;
    let v: Value = resp
        .into_json()
        .map_err(|e| format!("账号中心返回的不是 JSON:{e}"))?;
    crate::collab::interconnect::apply_received_relays(&v)?;
    let key = v
        .get("meshKey")
        .and_then(|x| x.as_str())
        .ok_or("账号中心没返回设备密钥")?
        .to_string();
    let bound_assertion = fresh_assertion(&Cfg { url: url.into(), key: key.clone() })?;
    let (_, bound_token) = if console {
        crate::collab::authority::login_with_assertion_local(&bound_assertion, node_id)?
    } else {
        crate::collab::authority::login_with_assertion(&bound_assertion, node_id)?
    };
    crate::collab::auth::logout(&token)?;
    let token = bound_token;
    let uid = v
        .get("uid")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();

    Ok(json!({
        "key": key,
        "uid": uid,
        "pinWarn": "",
        "memberRole": user.role,
        "token": token,
        "memberWarn": "",
    }))
}

/// 入网成功后的落库 + 起循环。同步小操作,单独抽出来是为了让两条登录路共用同一份收尾。
pub(super) fn finish_enroll(url: &str, node_id: &str, out: &Value) -> Result<Value, String> {
    let key = out["key"]
        .as_str()
        .filter(|v| !v.is_empty())
        .ok_or("入网结果缺少设备密钥，未保存登录状态")?;
    let uid = out["uid"]
        .as_str()
        .filter(|v| !v.is_empty())
        .ok_or("入网结果缺少账号 UID，未保存登录状态")?;
    crate::collab::db::meta_set_many(&[(K_URL, url), (K_KEY, key), (K_UID, uid), ("mesh_node_id", node_id)])?;
    Ok(json!({
        "enrolled": true,
        "url": url,
        "uid": out["uid"],
        "email": out.get("email").cloned().unwrap_or(json!("")),
        "nodeId": node_id,
        "token": out["token"],
        // 两条「半成品」警告如实上报,UI 据此告诉用户「还差一步」而不是假装全好了:
        //  · pinWarn   :本机之前信任着**另一个**账号中心 → 别的设备连不进来;
        //  · memberWarn:这个账号还没成为本机成员(比如本机已有同名本地账号)→ 同上。
        "pinWarn": out["pinWarn"],
        "memberWarn": out["memberWarn"],
        "memberRole": out["memberRole"],
    }))
}

/// 入网地址:调用方填了就用它,没填看环境变量(server 形态),最后落到官方云机。
fn authority_of(url: Option<String>) -> Result<String, String> {
    let config = crate::collab::interconnect::load()?;
    if std::env::var_os("POLARIS_ACCOUNT_AUTHORITY_URL").is_some() {
        return Ok(config.authority.url.map_or_else(String::new, |url| url));
    }
    Ok(url.filter(|url| !url.trim().is_empty()).or(config.authority.url)
        .map_or_else(String::new, |url| url))
}

/// 拿一张**已有的**断言入网(手机壳/脚本等已经自己登过的场景)。
/// 桌面 UI 走的是 [`account_login_code`],不经过这里。
#[cfg_attr(feature = "desktop", tauri::command)]
#[allow(non_snake_case)]
pub async fn mesh_join(
    assertion: String,
    url: Option<String>,
    inviteCode: Option<String>,
    allowInsecureHttp: Option<bool>,
) -> Result<Value, String> {
    let node_id = my_node_id();
    if node_id.is_empty() {
        return Err("本机 P2P 身份生成失败(数据目录写不进去?)—— 换个数据目录再试".into());
    }
    let url = validate_authority_url(&authority_of(url)?, allowInsecureHttp.unwrap_or(false))?;
    let invite = inviteCode.unwrap_or_default();
    orchestration::complete_login(url, node_id, assertion, invite, false).await

}

/// Password and OTP login converge on the same enrollment/discovery/relay/connect sequence.
#[cfg_attr(feature = "desktop", tauri::command)]
#[allow(non_snake_case)]
pub async fn account_login_password(
    username: String, password: String, url: Option<String>,
    inviteCode: Option<String>, allowInsecureHttp: Option<bool>,
) -> Result<Value, String> {
    let url = validate_authority_url(&authority_of(url)?, allowInsecureHttp == Some(true))?;
    let node = my_node_id();
    if node.is_empty() { return Err("本机 P2P 身份尚未就绪".into()); }
    let endpoint = url.clone();
    let assertion = tokio::task::spawn_blocking(move || -> Result<String, String> {
        crate::collab::authority::pin_explicit(&endpoint)?;
        let response: Value = agent()?.post(&format!("{endpoint}/api/account/login"))
            .send_json(json!({"username": username, "password": password}))
            .map_err(cloud_err)?.into_json().map_err(|e| format!("账号中心 JSON 无效: {e}"))?;
        crate::collab::interconnect::apply_received_relays(&response)?;
        response["assertion"].as_str().map(String::from).ok_or_else(|| "账号中心缺少断言".into())
    }).await.map_err(|e| format!("密码登录任务失败: {e}"))??;
    orchestration::complete_login(
        url, node, assertion, inviteCode.map_or_else(String::new, |invite| invite), true,
    ).await
}


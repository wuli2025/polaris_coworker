//! Authority HTTP and transport policy.
use super::*;

// ────────────────────────────── 与云端目录通信 ──────────────────────────────

pub(super) fn agent() -> Result<ureq::Agent, String> {
    crate::collab::interconnect::http_client::agent(ureq::AgentBuilder::new()
        .timeout_connect(CLOUD_TIMEOUT)
        .timeout_read(CLOUD_TIMEOUT))
}

/// 把 ureq 错误翻成人话。云端明确给的业务错误原样透传(「设备已被移出设备网」这类
/// 必须让用户看见原文,糊成「网络错误」会让人对着正确的账号反复试)。
pub(super) fn cloud_err(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, resp) => {
            let body = resp.into_string().unwrap_or_default();
            serde_json::from_str::<Value>(&body)
                .ok()
                .and_then(|v| v.get("error").and_then(|x| x.as_str()).map(String::from))
                .unwrap_or_else(|| format!("HTTP {code}"))
        }
        error => crate::collab::interconnect::http_client::diagnostic(&error),
    }
}

pub(super) fn norm_url(u: &str) -> String {
    let t = u.trim().trim_end_matches('/');
    if t.starts_with("http://") || t.starts_with("https://") {
        t.to_string()
    } else {
        format!("https://{t}")
    }
}

fn loopback_http(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("http://") else {
        return false;
    };
    let authority = rest.split('/').next().unwrap_or("");
    let host = authority.rsplit('@').next().unwrap_or(authority);
    host == "localhost"
        || host.starts_with("localhost:")
        || host == "127.0.0.1"
        || host.starts_with("127.0.0.1:")
        || host == "[::1]"
        || host.starts_with("[::1]:")
}

/// 登录凭据、OTP、assertion 与长期 meshKey 都会经过账号中心。公网 HTTP 必须显式确认，
/// 不能再因为用户把“高级地址”留空，就静默把整套身份发到明文 IP。
pub(super) fn validate_authority_url(raw: &str, allow_insecure_http: bool) -> Result<String, String> {
    if raw.trim().is_empty() {
        return Err("未配置权威；当前可继续使用本地模式".into());
    }
    let url = norm_url(raw);
    if url == "https://" || url == "http://" {
        return Err("请填写账号中心地址".into());
    }
    if url.starts_with("https://") || loopback_http(&url) {
        return Ok(url);
    }
    if url.starts_with("http://") {
        if allow_insecure_http {
            return Ok(url);
        }
        return Err(format!(
            "账号中心 {url} 使用明文 HTTP，邮箱、验证码和设备密钥可能被窃取。请改用 HTTPS；若这是你明确信任的旧服务，请在高级设置勾选‘仍使用不安全 HTTP’后重试"
        ));
    }
    Err("账号中心地址只支持 https://（本机调试可用 http://localhost）".into())
}

pub(super) fn authority_transport_state(url: &str) -> &'static str {
    if url.starts_with("https://") || loopback_http(url) {
        "secure"
    } else {
        "insecure_legacy"
    }
}

/// 心跳 + 取同账号设备清单。
pub(super) fn announce(c: &Cfg) -> Result<Value, String> {
    crate::collab::authority::pin_explicit(&c.url)?;
    crate::collab::db::meta_set("mesh_node_id", &my_node_id())?;
    let resp = agent()?
        .post(&format!("{}/api/mesh/announce", c.url))
        .set("Authorization", &format!("Bearer {}", c.key))
        .send_json(json!({ "name": my_name(), "os": my_os(), "ver": my_ver() }))
        .map_err(cloud_err)?;
    let v: Value = resp
        .into_json()
        .map_err(|e| format!("云端返回的不是 JSON:{e}"))?;
    if let Some(uid) = v.get("uid").and_then(|x| x.as_str()) {
        let _ = crate::collab::db::meta_set(K_UID, uid);
    }
    security::publish(&v);
    Ok(v)
}

/// 取一张新的身份断言(5 分钟有效),用来进对端主机的门。
pub(super) fn fresh_assertion(c: &Cfg) -> Result<String, String> {
    crate::collab::authority::pin_explicit(&c.url)?;
    let resp = agent()?
        .post(&format!("{}/api/mesh/assert", c.url))
        .set("Authorization", &format!("Bearer {}", c.key))
        .send_json(json!({}))
        .map_err(cloud_err)?;
    let v: Value = resp
        .into_json()
        .map_err(|e| format!("云端返回的不是 JSON:{e}"))?;
    v.get("assertion")
        .and_then(|x| x.as_str())
        .map(String::from)
        .ok_or_else(|| "云端没返回身份断言".into())
}


//! Device credential adapter. All remote URLs come from validated server configuration.
use super::*;
use crate::collab::{authority as a, auth, mesh};

pub fn request(method: &str, url: &str, key: Option<&str>, body: &Value) -> Result<Value, String> {
    let agent = super::super::http_client::agent(ureq::AgentBuilder::new().redirects(0)
        .timeout(std::time::Duration::from_secs(super::super::load()?.browser.request_timeout_secs)))?;
    let mut request = agent.request(method, url);
    if let Some(key) = key { request = request.set("Authorization", &format!("Bearer {key}")); }
    let response = if method == "GET" { request.call() } else { request.send_json(body.clone()) };
    match response {
        Ok(response) => serde_json::from_reader(response.into_reader().take(2 * 1024 * 1024))
            .map_err(|_| "interconnect_invalid_response".into()),
        Err(ureq::Error::Status(status, response)) => {
            if status == 401 || status == 403 { return Err("interconnect_upstream_unauthorized".into()); }
            let value: Value = serde_json::from_reader(response.into_reader().take(65536))
                .map_err(|_| "interconnect_upstream_failed")?;
            // Only stable policy outcomes cross the transport boundary; never upstream diagnostics.
            let error = value["error"].as_str().map_or("", |value| value);
            if ["approval_pending:", "approval_expired_or_denied:"].iter().any(|prefix| error.starts_with(prefix)) {
                return Err(error.to_string());
            }
            Err("interconnect_upstream_failed".into())
        }
        Err(error) => Err(super::super::http_client::diagnostic(&error)),
    }
}

pub fn password(username: &str, password: &str) -> Result<(String, a::Claims), String> {
    let assertion = if a::is_authority() {
        let (user, uid, email) = auth::authority_login(username, password)?;
        a::sign_assertion(&uid, &user.username, &email, &user.display_name)?
    } else {
        let url = super::super::load()?.authority.url.ok_or("interconnect_authority_required")?;
        a::pin_explicit(&url)?;
        request("POST", &format!("{url}/api/account/login"), None,
            &json!({"username":username,"password":password}))?["assertion"]
            .as_str().ok_or("interconnect_missing_assertion")?.to_string()
    };
    let claims = verify(&assertion)?;
    Ok((assertion, claims))
}

pub fn verify(assertion: &str) -> Result<a::Claims, String> {
    if a::is_authority() { a::verify_assertion_with(assertion, &a::public_key_b64()?) }
    else { a::verify_assertion(assertion) }
}

pub fn enroll(assertion: &str, uid: &str, node: &str, name: &str, platform: &str) -> Result<String, String> {
    if a::is_authority() { return mesh::enroll(uid, node, name, platform, env!("CARGO_PKG_VERSION")); }
    let url = super::super::load()?.authority.url.ok_or("interconnect_authority_required")?;
    request("POST", &format!("{url}/api/mesh/enroll"), None,
        &json!({"assertion":assertion,"nodeId":node,"name":name,"os":platform,"ver":env!("CARGO_PKG_VERSION")}))?
        ["meshKey"].as_str().map(String::from).ok_or_else(|| "interconnect_missing_key".into())
}

pub fn call(record: &Record, method: &str, path: &str, value: &Value) -> Result<Value, String> {
    let current_url = if a::is_authority() { None } else { super::super::load()?.authority.url };
    if current_url != record.authority_url { return Err("interconnect_authority_changed".into()); }
    if !a::is_authority() {
        let url = current_url.ok_or("interconnect_authority_required")?;
        a::pin_explicit(&url)?;
        return request(method, &format!("{url}{path}"), Some(&record.key), value);
    }
    let (uid, node) = mesh::resolve_key(&record.key)?;
    if uid != record.uid || node != record.node { return Err("interconnect_device_mismatch".into()); }
    match path {
        "/api/mesh/assert" => Ok(json!({"assertion":a::sign_device_assertion(&uid, &node)?})),
        "/api/mesh/announce" => {
            let (_, peers) = mesh::announce(&record.key, &record.name, &record.platform, env!("CARGO_PKG_VERSION"))?;
            Ok(json!({"peers":peers,"relays":super::super::advertised_relays()?}))
        }
        "/api/mesh/security/inbox" => Ok(json!({
            "notifications":super::super::notifications::inbox(&uid, &node)?,
            "approvals":super::super::risk::pending(&uid, &node)?,
        })),
        "/api/mesh/security/ack" => {
            super::super::notifications::acknowledge(&uid, &node, string(value, "deviceId")?)?;
            Ok(json!({"ok":true}))
        }
        "/api/mesh/risk/decide" => {
            super::super::risk::decide(&uid, &node, string(value, "requestId")?,
                value["approve"].as_bool().ok_or("approve must be boolean")?)?;
            Ok(json!({"ok":true}))
        }
        "/api/mesh/revoke" => {
            super::super::gate::require(&uid, &node, "revoke_other_device", value)?;
            mesh::revoke(&uid, string(value, "nodeId")?)?;
            Ok(json!({"ok":true}))
        }
        "/api/mesh/risk/authorize" => {
            let permit = super::super::risk::authorize(&uid, &node, &node,
                string(value, "operation")?, string(value, "digest")?, string(value, "nonce")?)?;
            Ok(json!({"permit":a::sign_document("PRISK1", &permit)?}))
        }
        _ => Err("interconnect_unsupported_operation".into()),
    }
}

use std::io::Read;

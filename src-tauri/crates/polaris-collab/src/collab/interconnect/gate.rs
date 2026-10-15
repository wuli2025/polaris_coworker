//! Shared high-risk adapter for HTTP actors and the local mesh console.
use crate::collab::{authority, db};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

pub fn require(uid: &str, actor_node: &str, operation: &str, payload: &Value) -> Result<(), String> {
    require_proven(uid, actor_node, operation, payload, None)
}

pub fn require_proven(
    uid: &str, actor_node: &str, operation: &str, payload: &Value, assertion: Option<&str>,
) -> Result<(), String> {
    if uid.is_empty() { return Ok(()); }
    let digest = super::risk::digest(operation, payload);
    let nonce = crate::collab::auth::random_token();
    if authority::is_authority() {
        super::risk::authorize(uid, actor_node, actor_node, operation, &digest, &nonce)?;
        return Ok(());
    }
    let snapshot = super::revocations::cached(uid)?;
    let actor = snapshot.devices.iter().find(|device| device.node_id == actor_node)
        .ok_or("需要联网确认：最新清单尚未包含当前设备")?;
    if super::revocations::is_revoked(uid, &actor.device_id)? { return Err("设备已吊销".into()); }
    let window = super::load()?.security.new_device_window_hours as i64 * 3600;
    if snapshot.devices.len() == 1 || snapshot.first_device.as_deref() == Some(&actor.device_id)
        || db::now() >= actor.enrolled_at + window
    { return Ok(()); }
    let (url, key, _) = authority::pinned().ok_or("需要联网确认：尚未信任权威")?;
    let mesh_key = db::meta_get("mesh_key").ok_or("需要联网确认：本机尚未入网")?;
    let response = ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(10)).build()
        .post(&format!("{url}/api/mesh/risk/authorize"))
        .set("Authorization", &format!("Bearer {mesh_key}"))
        .send_json(json!({"actorNodeId":actor_node, "operation":operation, "digest":digest, "nonce":nonce, "actorAssertion":assertion}));
    let response: Value = match response {
        Ok(response) => response.into_json().map_err(|e| e.to_string())?,
        Err(ureq::Error::Status(_, response)) => {
            let value: Value = response.into_json().map_err(|e| e.to_string())?;
            return Err(value["error"].as_str().map_or_else(|| "需要联网确认".into(), String::from));
        }
        Err(error) => return Err(format!("需要联网确认：{error}")),
    };
    let document = response["permit"].as_str().ok_or("需要联网确认：权威未返回签名批准")?;
    let config = super::load()?;
    if !config.authority.public_key_fingerprints.is_empty() { config.verify_key(&key)?; }
    let permit: super::risk::Permit = authority::verify_document("PRISK1", document, &key)?;
    let host = db::meta_get("mesh_node_id").map_or_else(String::new, |v| v);
    if permit.uid != uid || permit.digest != digest || permit.nonce != nonce
        || permit.expires_at <= db::now() || permit.host != host
        || actor.device_id != permit.actor
    { return Err("高风险批准与当前设备、主机或操作不匹配".into()); }
    Ok(())
}

/// Legacy assertions retain their prior policy; bound sessions must use the approval gate.
pub fn session(token: &str, operation: &str, payload: &Value) -> Result<(), String> {
    session_proven(token, operation, payload, None)
}

pub fn session_proven(token: &str, operation: &str, payload: &Value, assertion: Option<&str>) -> Result<(), String> {
    crate::collab::auth::check_session(token)?;
    let binding: Option<(String, String)> = db::with_conn(|conn| conn.query_row(
        "SELECT uid,node_id FROM bound_sessions WHERE token=?1", params![token],
        |r| Ok((r.get(0)?, r.get(1)?)),
    ).optional().map_err(|e| e.to_string()))?;
    if let Some((uid, node)) = binding { require_proven(&uid, &node, operation, payload, assertion)?; }
    Ok(())
}

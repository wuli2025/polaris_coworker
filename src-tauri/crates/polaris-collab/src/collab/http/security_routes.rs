//! Authority notification/approval routes. Actor identities always come from mesh credentials.
use super::*;
use crate::collab::{authority, interconnect, mesh};

/// Long polling delivers persisted notifications as soon as enrollment commits.
/// Cursor avoids repeatedly returning the same unacknowledged inbox in a tight loop.
pub(super) async fn security_watch(headers: HeaderMap, Query(query): Query<HashMap<String, String>>) -> Response {
    if !authority::is_authority() { return not_authority(); }
    let changed = interconnect::notifications::CHANGED.notified();
    tokio::pin!(changed);
    changed.as_mut().enable();
    let response = security_inbox(headers.clone()).await;
    if !response.status().is_success() { return response; }
    let bytes = match axum::body::to_bytes(response.into_body(), 2 * 1024 * 1024).await {
        Ok(bytes) => bytes,
        Err(error) => return err_resp(error.to_string()),
    };
    let mut inbox: Value = match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(error) => return err_resp(error.to_string()),
    };
    let cursor = interconnect::risk::digest("inbox", &inbox);
    if query.get("cursor") != Some(&cursor) {
        inbox["cursor"] = json!(cursor);
        return Json(inbox).into_response();
    }
    let timeout = match interconnect::load() {
        Ok(config) => Duration::from_secs(config.security.revocation_poll_secs),
        Err(error) => return err_resp(error),
    };
    let _ = tokio::time::timeout(timeout, changed).await;
    let response = security_inbox(headers).await;
    if !response.status().is_success() { return response; }
    let bytes = match axum::body::to_bytes(response.into_body(), 2 * 1024 * 1024).await {
        Ok(bytes) => bytes,
        Err(error) => return err_resp(error.to_string()),
    };
    let mut inbox: Value = match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(error) => return err_resp(error.to_string()),
    };
    inbox["cursor"] = json!(interconnect::risk::digest("inbox", &inbox));
    Json(inbox).into_response()
}

pub(super) async fn security_inbox(headers: HeaderMap) -> Response {
    security_call(headers, json!({}), |uid, node, _| Ok(json!({
        "notifications":interconnect::notifications::inbox(uid, node)?,
        "approvals":interconnect::risk::pending(uid, node)?,
    }))).await
}

pub(super) async fn security_ack(headers: HeaderMap, Json(value): Json<Value>) -> Response {
    security_call(headers, value, |uid, node, value| {
        interconnect::notifications::acknowledge(uid, node, &s_of(value, "deviceId"))?;
        Ok(json!({"ok":true}))
    }).await
}

pub(super) async fn security_decide(headers: HeaderMap, Json(value): Json<Value>) -> Response {
    security_call(headers, value, |uid, node, value| {
        let approved = value["approve"].as_bool().ok_or("approve 须为布尔值")?;
        interconnect::risk::decide(uid, node, &s_of(value, "requestId"), approved)?;
        Ok(json!({"ok":true}))
    }).await
}

pub(super) async fn security_authorize(headers: HeaderMap, Json(value): Json<Value>) -> Response {
    security_call(headers, value, |uid, host, value| {
        let actor_node = s_of(value, "actorNodeId");
        let actor_uid = if let Some(assertion) = value["actorAssertion"].as_str() {
            let claims = authority::verify_assertion_with(assertion, &authority::public_key_b64()?)?;
            let actor = interconnect::devices::active(&claims.uid, &actor_node)?;
            if claims.node_id.as_deref() != Some(actor_node.as_str())
                || claims.device_id.as_deref() != Some(actor.device_id.as_str())
            { return Err("高风险断言与请求设备不匹配".into()); }
            claims.uid
        } else { uid.into() };
        let permit = interconnect::risk::authorize_on(&actor_uid, &actor_node, uid, host,
            &s_of(value, "operation"), &s_of(value, "digest"), &s_of(value, "nonce"))?;
        Ok(json!({"permit":authority::sign_document("PRISK1", &permit)?}))
    }).await
}

async fn security_call(
    headers: HeaderMap, value: Value,
    call: fn(&str, &str, &Value) -> Result<Value, String>,
) -> Response {
    if !authority::is_authority() { return not_authority(); }
    let Some(key) = bearer_of(&headers) else { return forbid(); };
    unwrap_api(tokio::task::spawn_blocking(move || {
        let (uid, node) = mesh::resolve_key(&key)?;
        call(&uid, &node, &value)
    }).await)
}

/// Route-level gate covers mutations before their existing role and membership checks.
pub(super) async fn risk_middleware(
    request: axum::extract::Request, next: axum::middleware::Next,
) -> Response {
    let path = request.uri().path();
    let operation = interconnect::risk_inventory::HIGH_RISK_OPERATIONS.iter()
        .find(|op| op.entry_points.iter().any(|entry| entry.strip_prefix("POST ") == Some(path)))
        .map(|op| op.id);
    let Some(operation) = operation.filter(|_| request.method() == axum::http::Method::POST) else {
        return next.run(request).await;
    };
    // Mesh revoke has a device-key actor and is gated in its handler.
    if path == "/api/mesh/revoke" { return next.run(request).await; }
    let token = bearer_of(request.headers());
    let uri = request.uri().to_string();
    let assertion = request.headers().get("x-polaris-device-assertion")
        .and_then(|value| value.to_str().ok()).map(String::from);
    let (parts, body) = request.into_parts();
    let bytes = match axum::body::to_bytes(body, 2 * 1024 * 1024).await {
        Ok(bytes) => bytes,
        Err(error) => return err_resp(error.to_string()),
    };
    let value: Value = if bytes.is_empty() { json!({}) } else { match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(error) => return err_resp(error.to_string()),
    }};
    let result = tokio::task::spawn_blocking(move || {
        if uri.starts_with("/api/account/reset") || uri.starts_with("/api/collab/email/reset") {
            let email = s_of(&value, "email");
            let uid: String = db_uid_for_email(&email)?;
            if !mesh::devices(&uid)?.is_empty() {
                let key = token.as_deref().ok_or("需要联网确认：改密须提供已登记设备凭据")?;
                let (actor_uid, node) = mesh::resolve_key(key)?;
                if actor_uid != uid { return Err("改密账号与设备账号不匹配".into()); }
                return interconnect::gate::require(&uid, &node, operation, &value);
            }
        }
        let payload = json!({"uri":uri,"body":value});
        if let Some(token) = token {
            if crate::collab::auth::check_session(&token).is_ok() {
                return interconnect::gate::session_proven(&token, operation, &payload, assertion.as_deref());
            }
        }
        // Accounts without enrolled devices and machine-admin credentials retain local recovery.
        Ok(())
    }).await;
    match result {
        Ok(Ok(())) => next.run(axum::extract::Request::from_parts(parts, Body::from(bytes))).await,
        Ok(Err(error)) => fs_deny(error),
        Err(error) => err_resp(error.to_string()),
    }
}

fn db_uid_for_email(email: &str) -> Result<String, String> {
    use rusqlite::OptionalExtension;
    crate::collab::db::with_conn(|conn| conn.query_row(
        "SELECT uid FROM users WHERE email=lower(trim(?1))", rusqlite::params![email], |r| r.get(0),
    ).optional().map(|uid: Option<String>| uid.map_or_else(String::new, |uid| uid))
        .map_err(|e| e.to_string()))
}

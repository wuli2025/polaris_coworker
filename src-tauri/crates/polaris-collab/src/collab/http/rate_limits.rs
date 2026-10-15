//! Persistent request budgets before authentication, long polling or risk side effects.
//! Reuses the login/mail SQLite gate; serialize its read/update within this process.
use axum::{extract::{ConnectInfo, Request}, http::StatusCode, middleware::Next,
    response::{IntoResponse, Response}, Json};
use crate::collab::{db, interconnect};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::net::SocketAddr;

static GATE: parking_lot::Mutex<()> = parking_lot::Mutex::new(());
const LIMITED: &str = "interconnect_rate_limited";

fn category(path: &str) -> Option<&'static str> {
    match path {
        "/api/mesh/security/inbox" | "/api/mesh/security/watch" | "/api/mesh/revocations"
        | "/api/mesh/devices" | "/api/mesh/announce" | "/api/mesh/assert" => Some("read"),
        "/api/mesh/security/ack" | "/api/mesh/risk/decide" | "/api/mesh/risk/authorize"
        | "/api/mesh/revoke" | "/api/mesh/rename" | "/api/peer/revoke"
        | "/api/collab/admin/device_revoke" | "/api/account/reset"
        | "/api/collab/email/reset" => Some("write"),
        "/api/account/login" | "/api/account/login_code" | "/api/account/signup"
        | "/api/mesh/enroll" | "/api/collab/login_assertion" | "/api/collab/login"
        | "/api/collab/email/login" | "/api/collab/join" => Some("login"),
        _ => None,
    }
}

fn check(path: &str, source: &str, credential: Option<&str>) -> Result<(), String> {
    let Some(kind) = category(path) else { return Ok(()); };
    let config = interconnect::load()?.rate_limit;
    let cap = match kind {
        "read" => config.read_requests,
        "write" => config.write_requests,
        _ => config.login_requests,
    } as i64;
    let scope = format!("interconnect:{path}");
    let window = config.window_secs as i64;
    let _guard = GATE.lock();
    // Expire only our own rate records; do not touch the existing login/mail budgets.
    db::open_db()?.execute("DELETE FROM login_attempts WHERE scope=?1 AND last_at<?2",
        rusqlite::params![scope, db::now() - window]).map_err(|e| e.to_string())?;
    // A global ceiling bounds random credentials and shells without peer metadata within each window.
    db::rate_gate(&scope, "global", cap * config.global_multiplier as i64, window, LIMITED)?;
    db::rate_gate(&scope, &format!("source:{source}"),
        cap * config.global_multiplier as i64, window, LIMITED)?;
    let identity = credential.filter(|_| kind != "login").map(|raw| format!("credential:{}", hex::encode(Sha256::digest(raw))));
    db::rate_gate(&scope, identity.as_deref().map_or(source, |key| key), cap, window, LIMITED)
}

pub(super) async fn middleware(request: Request, next: Next) -> Response {
    let path = request.uri().path().to_string();
    if category(&path).is_none() { return next.run(request).await; }
    // Never trust caller-controlled X-Forwarded-For. Shells without ConnectInfo share a bounded bucket.
    let source = request.extensions().get::<ConnectInfo<SocketAddr>>()
        .map_or_else(|| "unknown-peer".to_string(), |peer| peer.0.ip().to_string());
    let credential = request.headers().get("authorization").and_then(|v| v.to_str().ok())
        .filter(|value| value.starts_with("Bearer ")).map(String::from);
    let result = tokio::task::spawn_blocking(move || check(&path, &source, credential.as_deref())).await;
    match result {
        Ok(Ok(())) => next.run(request).await,
        Ok(Err(error)) if error == LIMITED => (StatusCode::TOO_MANY_REQUESTS,
            Json(json!({"error":LIMITED}))).into_response(),
        Ok(Err(error)) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error":error}))).into_response(),
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":error.to_string()}))).into_response(),
    }
}

#[cfg(test)]
#[path = "rate_limit_tests.rs"]
mod tests;

//! Session-scoped HTTP orchestration. No caller-selected host identity or console admission.
use super::*;
use crate::collab::interconnect::browser;

pub(super) async fn browser_login(Json(value): Json<Value>) -> Response {
    match browser::password_login(value).await {
        Ok(value) => Json(value).into_response(),
        Err(_) => login_failed(),
    }
}
fn login_failed() -> Response {
    (StatusCode::UNAUTHORIZED, Json(json!({"error":"interconnect_login_failed",
        "interconnect":{"stage":"register","state":"failed","error":"interconnect_login_failed","retryable":true}})))
        .into_response()
}
pub(super) async fn browser_status(headers: HeaderMap) -> Response {
    let Some(token) = bearer_of(&headers) else { return forbid(); };
    match tokio::task::spawn_blocking(move || browser::status(&token)).await {
        Ok(Ok(value)) => Json(value).into_response(),
        _ => forbid(),
    }
}
pub(super) async fn browser_retry(headers: HeaderMap) -> Response {
    let Some(token) = bearer_of(&headers) else { return forbid(); };
    let checked = token.clone();
    if !matches!(tokio::task::spawn_blocking(move || browser::status(&checked)).await, Ok(Ok(_))) { return forbid(); }
    match browser::retry(token).await {
        Ok(value) => Json(value).into_response(),
        Err(error) if error == "interconnect_retry_busy" =>
            (StatusCode::CONFLICT, Json(json!({"error":error}))).into_response(),
        Err(_) => (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error":"interconnect_retry_failed"}))).into_response(),
    }
}
async fn security(headers: HeaderMap, path: &'static str, value: Value) -> Response {
    let Some(token) = bearer_of(&headers) else { return forbid(); };
    let checked = token.clone();
    if !matches!(tokio::task::spawn_blocking(move || browser::status(&checked)).await, Ok(Ok(_))) { return forbid(); }
    unwrap_api(tokio::task::spawn_blocking(move || browser::security(&token, path, &value)).await)
}
pub(super) async fn browser_inbox(headers: HeaderMap) -> Response {
    security(headers, "/api/mesh/security/inbox", json!({})).await
}
pub(super) async fn browser_ack(headers: HeaderMap, Json(value): Json<Value>) -> Response {
    security(headers, "/api/mesh/security/ack", value).await
}
pub(super) async fn browser_decide(headers: HeaderMap, Json(value): Json<Value>) -> Response {
    security(headers, "/api/mesh/risk/decide", value).await
}
pub(super) async fn browser_revoke(headers: HeaderMap, Json(value): Json<Value>) -> Response {
    security(headers, "/api/mesh/revoke", value).await
}

/// Sessions and downstream credentials must never enter browser or intermediary caches.
pub(super) async fn browser_no_store(request: axum::extract::Request, next: axum::middleware::Next) -> Response {
    let protected = request.uri().path().starts_with("/api/interconnect/");
    let mut response = next.run(request).await;
    if protected { response.headers_mut().insert(header::CACHE_CONTROL, axum::http::HeaderValue::from_static("no-store")); }
    response
}

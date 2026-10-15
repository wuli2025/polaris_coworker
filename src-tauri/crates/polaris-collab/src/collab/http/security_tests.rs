use super::*;
use crate::collab::{authority, db, interconnect, mesh};
use tower::ServiceExt;

fn setup() -> Result<interconnect::test_env::TestEnv, String> {
    let env = interconnect::test_env::TestEnv::new("security-http")?;
    std::env::set_var("POLARIS_ACCOUNT_AUTHORITY", "1");
    db::open_db()?.execute(
        "INSERT INTO users(id,username,pass_hash,role,display_name,created_at,email,uid)
         VALUES(1,'alice','!federated','owner','Alice',1,'alice@example.com','acct_test')", [],
    ).map_err(|e| e.to_string())?;
    db::meta_set_many(&[("owner_uid", "acct_test"), ("authority_url", "https://id.example.com"),
        ("authority_pub", &authority::public_key_b64()?)])?;
    Ok(env)
}

fn headers(key: &str) -> Result<HeaderMap, String> {
    let mut headers = HeaderMap::new();
    headers.insert("authorization", format!("Bearer {key}").parse().map_err(|e| format!("{e}"))?);
    Ok(headers)
}

async fn value(response: Response) -> Result<Value, String> {
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.map_err(|e| e.to_string())?;
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}

#[tokio::test]
async fn authority_wire_binds_assertions_delivers_notifications_and_gates_revoke() -> Result<(), String> {
    let _env = setup()?;
    let first = mesh::enroll("acct_test", "node-a", "First", "linux", "1")?;
    let second = mesh::enroll("acct_test", "node-b", "Second", "windows", "1")?;
    let asserted = value(mesh_assert(headers(&second)?).await).await?;
    let claims = authority::verify_assertion_with(asserted["assertion"].as_str().ok_or("no assertion")?,
        &authority::public_key_b64()?)?;
    assert_eq!(claims.device_id, Some(interconnect::devices::active("acct_test", "node-b")?.device_id));
    assert_eq!(claims.node_id.as_deref(), Some("node-b"));
    assert_eq!(claims.exp - claims.iat, 300);
    let inbox = value(mesh_announce(headers(&first)?, Json(json!({}))).await).await?;
    assert_eq!(inbox["notifications"][0]["name"], "Second");
    let pending = value(mesh_revoke(headers(&second)?, Json(json!({"nodeId":"node-a"}))).await).await?;
    assert!(pending["error"].as_str().is_some_and(|error| error.starts_with("approval_pending:")));
    assert!(mesh::resolve_key(&first).is_ok(), "no side effect while approval is pending");
    let inbox = value(security_inbox(headers(&first)?).await).await?;
    let request = inbox["approvals"][0]["id"].as_str().ok_or("no approval request")?;
    let approved = value(security_decide(headers(&first)?, Json(json!({
        "requestId":request,"approve":true,
    }))).await).await?;
    assert_eq!(approved["ok"], true);
    let result = value(mesh_revoke(headers(&second)?, Json(json!({"nodeId":"node-a"}))).await).await?;
    assert_eq!(result["ok"], true);
    assert!(mesh::resolve_key(&first).is_err());
    let document = value(mesh_revocations(Query(HashMap::from([
        ("uid".into(), "acct_test".into()),
    ]))).await).await?;
    interconnect::revocations::accept("acct_test", document["document"].as_str().ok_or("no signed list")?)?;
    Ok(())
}

#[tokio::test]
async fn http_mutation_is_suspended_until_approved_and_query_delete_needs_no_json_body() -> Result<(), String> {
    let _env = setup()?;
    mesh::enroll("acct_test", "node-a", "First", "linux", "1")?;
    mesh::enroll("acct_test", "node-b", "Second", "linux", "1")?;
    let assertion = authority::sign_device_assertion("acct_test", "node-b")?;
    let (_, token) = authority::login_with_assertion(&assertion, "node-b")?;
    let router = Router::new()
        .route("/api/fs/op/delete", post(|| async { Json(json!({"mutated":true})) }))
        .layer(axum::middleware::from_fn(risk_middleware));
    let request = || axum::http::Request::builder().method("POST")
        .uri("/api/fs/op/delete?path=important.txt")
        .header("Authorization", format!("Bearer {token}")).body(Body::empty())
        .map_err(|e| e.to_string());
    let response = router.clone().oneshot(request()?).await.map_err(|e| e.to_string())?;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let pending = interconnect::risk::pending("acct_test", "node-a")?;
    let id = pending.first().and_then(|v| v["id"].as_str()).ok_or("no pending request")?;
    interconnect::risk::decide("acct_test", "node-a", id, true)?;
    let response = router.oneshot(request()?).await.map_err(|e| e.to_string())?;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(value(response).await?["mutated"], true);
    Ok(())
}

#[tokio::test]
async fn watching_device_receives_enrollment_without_waiting_for_heartbeat() -> Result<(), String> {
    let _env = setup()?;
    let first = mesh::enroll("acct_test", "node-a", "First", "linux", "1")?;
    let initial = value(security_watch(headers(&first)?, Query(HashMap::new())).await).await?;
    let cursor = initial["cursor"].as_str().ok_or("missing cursor")?.to_string();
    let watch = tokio::spawn(security_watch(headers(&first)?, Query(HashMap::from([("cursor".into(), cursor)]))));
    tokio::task::yield_now().await;
    mesh::enroll("acct_test", "node-b", "Second", "windows", "1")?;
    let response = tokio::time::timeout(Duration::from_secs(2), watch).await
        .map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
    assert_eq!(value(response).await?["notifications"][0]["name"], "Second");
    Ok(())
}

#[tokio::test]
async fn cross_account_host_requires_fresh_actor_assertion() -> Result<(), String> {
    let _env = setup()?;
    mesh::enroll("acct_test", "node-a", "First", "linux", "1")?;
    mesh::enroll("acct_test", "node-b", "Second", "linux", "1")?;
    let host_key = mesh::enroll("acct_host", "node-host", "Host", "linux", "1")?;
    let mut request = json!({"actorNodeId":"node-b", "operation":"delete_account",
        "digest":interconnect::risk::digest("delete_account", &json!({"id":1})), "nonce":"unique"});
    let denied = value(security_authorize(headers(&host_key)?, Json(request.clone())).await).await?;
    assert!(denied["error"].as_str().is_some_and(|error| error.contains("未登记")));
    request["actorAssertion"] = json!(authority::sign_device_assertion("acct_test", "node-b")?);
    let pending = value(security_authorize(headers(&host_key)?, Json(request.clone())).await).await?;
    assert!(pending["error"].as_str().is_some_and(|error| error.starts_with("approval_pending:")));
    let list = interconnect::risk::pending("acct_test", "node-a")?;
    let id = list.first().and_then(|v| v["id"].as_str()).ok_or("no request")?;
    interconnect::risk::decide("acct_test", "node-a", id, true)?;
    let approved = value(security_authorize(headers(&host_key)?, Json(request)).await).await?;
    let permit: interconnect::risk::Permit = authority::verify_document("PRISK1",
        approved["permit"].as_str().ok_or("no permit")?, &authority::public_key_b64()?)?;
    assert_eq!(permit.uid, "acct_test");
    assert_eq!(permit.host, "node-host");
    Ok(())
}

use super::*;
use crate::collab::{auth, authority, db, interconnect, mesh};
use interconnect::test_env::TestEnv;
use tower::ServiceExt;

fn setup() -> Result<(TestEnv, String, Router), String> {
    let env = TestEnv::new("browser-http")?;
    std::env::set_var("POLARIS_ACCOUNT_AUTHORITY", "1");
    let (user, uid) = auth::create_authority_account("webuser", "test-password-123", "Web user", "")?;
    db::with_conn(|conn| conn.execute("UPDATE users SET role='owner' WHERE id=?1", [user.id])
        .map(|_| ()).map_err(|e| e.to_string()))?;
    let (sender, _) = tokio::sync::broadcast::channel(4);
    let router = collab_router(CollabState { app: crate::host::AppHandle::new(sender),
        auth_token: Arc::new(None), advertise: Arc::new(parking_lot::RwLock::new(vec![])) }, false);
    Ok((env, uid, router))
}
async fn call(router: &Router, method: &str, path: &str, token: &str, value: Value) -> Result<(StatusCode, Value), String> {
    let mut request = axum::http::Request::builder().method(method).uri(path).header("content-type", "application/json");
    if !token.is_empty() { request = request.header("authorization", format!("Bearer {token}")); }
    let response = router.clone().oneshot(request.body(Body::from(value.to_string())).map_err(|e| e.to_string())?)
        .await.map_err(|e| e.to_string())?;
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 2 * 1024 * 1024).await.map_err(|e| e.to_string())?;
    Ok((status, serde_json::from_slice(&bytes).map_err(|e| e.to_string())?))
}
async fn login(router: &Router) -> Result<Value, String> {
    let (status, value) = call(router, "POST", "/api/interconnect/login", "", json!({
        "username":"webuser","password":"test-password-123","deviceName":"My browser","platform":"web",
    })).await?;
    assert_eq!(status, StatusCode::OK, "{value}");
    Ok(value)
}
fn field<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value[key].as_str().ok_or_else(|| format!("missing {key}"))
}

#[tokio::test]
async fn login_is_bound_queryable_and_does_not_claim_the_console() -> Result<(), String> {
    let (_env, uid, router) = setup()?;
    db::meta_set("owner_uid", "existing-owner")?;
    let output = login(&router).await?;
    assert_eq!(output["interconnect"]["state"], "ready");
    assert_eq!(output["storageMode"], "http");
    assert!(output.get("relayConfig").is_some());
    let token = field(&output, "token")?;
    let binding: (String, String) = db::with_conn(|conn| conn.query_row(
        "SELECT uid,node_id FROM bound_sessions WHERE token=?1", [token], |r| Ok((r.get(0)?,r.get(1)?)))
        .map_err(|e| e.to_string()))?;
    assert_eq!(binding.0, uid);
    assert_eq!(binding.1, output["nodeId"]);
    assert_eq!(authority::owner_uid(), "existing-owner", "HTTP must preserve host ownership");
    assert!(mesh::peers(&uid, "another-host")?.is_empty(), "session devices must not be dialed as iroh nodes");
    let (status, queried) = call(&router, "GET", "/api/interconnect/status", token, json!({})).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(queried["interconnect"], output["interconnect"]);
    assert!(queried.get("token").is_none());
    let second = login(&router).await?;
    assert_ne!(second["nodeId"], output["nodeId"]);
    assert_eq!(call(&router, "GET", "/api/interconnect/status", "", json!({})).await?.0, StatusCode::FORBIDDEN);
    mesh::revoke(&uid, field(&output, "nodeId")?)?;
    assert_eq!(call(&router, "POST", "/api/interconnect/retry", token, json!({})).await?.0, StatusCode::FORBIDDEN);
    Ok(())
}

#[tokio::test]
async fn failed_connect_retry_preserves_device_key_and_registration_time() -> Result<(), String> {
    let (_env, uid, router) = setup()?;
    mesh::enroll(&uid, "invalid-iroh-node", "Existing host", "linux", "1")?;
    let output = login(&router).await?;
    assert_eq!(output["enrolled"], true);
    assert_eq!(output["interconnect"]["stage"], "connect");
    assert_eq!(output["interconnect"]["state"], "failed");
    let token = field(&output, "token")?;
    let node = field(&output, "nodeId")?;
    let before = interconnect::devices::active(&uid, node)?;
    let hash: String = db::with_conn(|conn| conn.query_row("SELECT key_hash FROM mesh_nodes WHERE node_id=?1", [node], |r| r.get(0))
        .map_err(|e| e.to_string()))?;
    db::with_conn(|conn| conn.execute("UPDATE mesh_nodes SET last_seen=0 WHERE node_id='invalid-iroh-node'", [])
        .map(|_| ()).map_err(|e| e.to_string()))?;
    let (status, retried) = call(&router, "POST", "/api/interconnect/retry", token, json!({})).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(retried["interconnect"]["state"], "ready");
    assert_eq!(retried["peers"][0]["state"], "offline");
    let after = interconnect::devices::active(&uid, node)?;
    assert_eq!(before.device_id, after.device_id);
    assert_eq!(before.enrolled_at, after.enrolled_at);
    let unchanged: bool = db::with_conn(|conn| conn.query_row(
        "SELECT key_hash=?2 FROM mesh_nodes WHERE node_id=?1", rusqlite::params![node,hash], |r| r.get(0))
        .map_err(|e| e.to_string()))?;
    assert!(unchanged);
    assert_eq!(mesh::devices(&uid)?.len(), 2);
    Ok(())
}

#[tokio::test]
async fn login_errors_do_not_reveal_missing_disabled_or_wrong_password_accounts() -> Result<(), String> {
    let (_env, _uid, router) = setup()?;
    let mut responses = vec![];
    for username in ["webuser", "missing-user"] {
        responses.push(call(&router, "POST", "/api/interconnect/login", "", json!({"username":username,"password":"wrong"})).await?);
    }
    db::with_conn(|conn| conn.execute("UPDATE users SET disabled=1", []).map(|_| ()).map_err(|e| e.to_string()))?;
    responses.push(call(&router, "POST", "/api/interconnect/login", "", json!({"username":"webuser","password":"test-password-123"})).await?);
    for response in &responses {
        assert_eq!(response, &responses[0]);
        assert_eq!(response.0, StatusCode::UNAUTHORIZED);
        assert_eq!(response.1["error"], "interconnect_login_failed");
    }
    Ok(())
}

#[tokio::test]
async fn new_browser_device_notifies_old_device_and_cannot_bypass_or_self_approve_risk() -> Result<(), String> {
    let (_env, uid, router) = setup()?;
    mesh::enroll(&uid, "old-host", "Old host", "linux", "1")?;
    let output = login(&router).await?;
    let token = field(&output, "token")?;
    let node = field(&output, "nodeId")?;
    let notices = interconnect::notifications::inbox(&uid, "old-host")?;
    assert_eq!(notices[0]["name"], "My browser");
    assert_eq!(notices[0]["platform"], "web");
    let operation = json!({"nodeId":"old-host"});
    let (status, pending) = call(&router, "POST", "/api/peer/revoke", token, operation.clone()).await?;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(field(&pending, "error")?.starts_with("approval_pending:"));
    assert!(!crate::collab::grants::is_revoked("old-host"));
    let inbox = interconnect::risk::pending(&uid, "old-host")?;
    let request = field(&inbox[0], "id")?;
    assert_eq!(inbox[0]["actorName"], "My browser");
    assert_eq!(inbox[0]["actorPlatform"], "web");
    assert!(interconnect::risk::decide(&uid, node, request, true).is_err());
    interconnect::risk::decide(&uid, "old-host", request, true)?;
    assert_eq!(call(&router, "POST", "/api/peer/revoke", token, operation).await?.0, StatusCode::OK);
    assert!(crate::collab::grants::is_revoked("old-host"));
    Ok(())
}

#[tokio::test]
async fn revoked_and_unknown_approval_actors_have_null_metadata() -> Result<(), String> {
    let (_env, uid, router) = setup()?;
    mesh::enroll(&uid, "old-host", "Old host", "linux", "1")?;
    let output = login(&router).await?;
    let token = field(&output, "token")?;
    call(&router, "POST", "/api/peer/revoke", token, json!({"nodeId":"target"})).await?;
    mesh::revoke(&uid, field(&output, "nodeId")?)?;
    let revoked = interconnect::risk::pending(&uid, "old-host")?;
    assert_eq!(revoked[0]["actorName"], Value::Null);
    assert_eq!(revoked[0]["actorPlatform"], Value::Null);
    db::with_conn(|conn| conn.execute("DELETE FROM mesh_device_ids WHERE node_id=?1", [field(&output, "nodeId")?])
        .map(|_| ()).map_err(|e| e.to_string()))?;
    let unknown = interconnect::risk::pending(&uid, "old-host")?;
    assert_eq!(unknown[0]["actorName"], Value::Null);
    assert_eq!(unknown[0]["actorPlatform"], Value::Null);
    Ok(())
}

#[tokio::test]
async fn http_login_status_and_retry_use_configured_budgets() -> Result<(), String> {
    let (env, _uid, router) = setup()?;
    std::fs::write(env.directory.join("config.json"), r#"{"rateLimit":{"loginRequests":2,"writeRequests":1,"readRequests":1}}"#)
        .map_err(|e| e.to_string())?;
    let output = login(&router).await?;
    let token = field(&output, "token")?;
    for (method, path) in [("GET", "/api/interconnect/status"), ("POST", "/api/interconnect/retry")] {
        assert_eq!(call(&router, method, path, token, json!({})).await?.0, StatusCode::OK);
        assert_eq!(call(&router, method, path, token, json!({})).await?.0, StatusCode::TOO_MANY_REQUESTS);
    }
    call(&router, "POST", "/api/interconnect/login", "random-1", json!({})).await?;
    let limited = call(&router, "POST", "/api/interconnect/login", "random-2", json!({})).await?;
    assert_eq!(limited.0, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(limited.1["error"], "interconnect_rate_limited");
    Ok(())
}

#[test]
fn browser_configuration_validates_types_limits_and_old_defaults() -> Result<(), String> {
    assert_eq!(interconnect::resolve(Some("{}"), |_| None)?.browser.request_timeout_secs, 10);
    for browser in [json!({"maxPeers":0}), json!({"requestTimeoutSecs":"10"}), json!({"platform":false}),
        json!({"deviceName":""}), json!({"maxPeers":129}), json!({"gatewayUrl":"https://user:pw@host"}), json!({"unknown":true})] {
        assert!(interconnect::resolve(Some(&json!({"browser":browser}).to_string()), |_| None).is_err());
    }
    Ok(())
}

#[tokio::test]
async fn existing_password_cooldown_still_applies_inside_orchestration() -> Result<(), String> {
    let (_env, _, router) = setup()?;
    auth::create_authority_account("cooldown-browser", "test-password-123", "Test", "")?;
    for _ in 0..5 {
        assert_eq!(call(&router, "POST", "/api/interconnect/login", "",
            json!({"username":"cooldown-browser","password":"wrong"})).await?.0, StatusCode::UNAUTHORIZED);
    }
    let (status, value) = call(&router, "POST", "/api/interconnect/login", "",
        json!({"username":"cooldown-browser","password":"test-password-123"})).await?;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(value["error"], "interconnect_login_failed");
    Ok(())
}

#[tokio::test]
async fn caller_cannot_select_an_old_device_or_console_mode_and_logout_removes_resume_state() -> Result<(), String> {
    let (_env, uid, router) = setup()?;
    for extra in [json!({"nodeId":"old-host"}), json!({"console":true}), json!({"url":"http://attacker"}), json!({"deviceName":false})] {
        let mut input = json!({"username":"webuser","password":"test-password-123"});
        for (key, value) in extra.as_object().ok_or("invalid fixture")? { input[key] = value.clone(); }
        assert_eq!(call(&router, "POST", "/api/interconnect/login", "", input).await?.0, StatusCode::UNAUTHORIZED);
    }
    assert!(mesh::devices(&uid)?.is_empty());
    let logged_in = login(&router).await?;
    let token = field(&logged_in, "token")?;
    assert_eq!(call(&router, "POST", "/api/collab/logout", token, json!({})).await?.0, StatusCode::OK);
    let remaining: i64 = db::with_conn(|conn| conn.query_row(
        "SELECT COUNT(*) FROM browser_interconnect WHERE token=?1", [token], |r| r.get(0)).map_err(|e| e.to_string()))?;
    assert_eq!(remaining, 0);
    assert_eq!(call(&router, "GET", "/api/interconnect/status", token, json!({})).await?.0, StatusCode::FORBIDDEN);
    Ok(())
}

#[tokio::test]
async fn browser_login_rejects_html_labels_and_accepts_chinese_names() -> Result<(), String> {
    let (_env, uid, router) = setup()?;
    for field in ["deviceName", "platform"] {
        for bad in ['<', '>', '"', '\'', '`'] {
            let mut input = json!({"username":"webuser","password":"test-password-123"});
            input[field] = json!(format!("设备{bad}名称"));
            let (status, value) = call(&router, "POST", "/api/interconnect/login", "", input).await?;
            assert_eq!(status, StatusCode::UNAUTHORIZED);
            assert_eq!(value["error"], "interconnect_login_failed");
        }
    }
    assert!(mesh::devices(&uid)?.is_empty());
    let name = "书房电脑-01_（测试） & + / 😀";
    let (status, value) = call(&router, "POST", "/api/interconnect/login", "",
        json!({"username":"webuser","password":"test-password-123","deviceName":name})).await?;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["devices"][0]["name"], name);
    assert_eq!(mesh::devices(&uid)?[0].0.name, name);
    Ok(())
}

#[tokio::test]
async fn telemetry_checks_full_name_before_existing_48_character_truncation() -> Result<(), String> {
    let (_env, _uid, router) = setup()?;
    let output = login(&router).await?;
    let token = field(&output, "token")?;
    let node = field(&output, "nodeId")?;
    let prefix = "中".repeat(48);
    for bad in ['<', '>', '"', '\'', '`', '\n'] {
        let (status, _) = call(&router, "POST", "/api/collab/telemetry", token,
            json!({"deviceId":node,"name":format!("{prefix}{bad}"),"stats":{}})).await?;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    let (status, _) = call(&router, "POST", "/api/collab/telemetry", token,
        json!({"deviceId":node,"name":format!("{prefix}尾"),"stats":{}})).await?;
    assert_eq!(status, StatusCode::OK);
    let stored = crate::collab::identity::list_devices()?;
    assert_eq!(stored.iter().find(|device| device.node_id == node).ok_or("missing device")?.name, prefix);
    Ok(())
}

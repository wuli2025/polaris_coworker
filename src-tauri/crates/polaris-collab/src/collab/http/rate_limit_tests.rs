use super::*;
use axum::{body::Body, http::Request, routing::any, Router};
use crate::collab::interconnect::test_env::TestEnv;
use tower::ServiceExt;

fn setup() -> Result<TestEnv, String> {
    let env = TestEnv::new("p6b-rate")?;
    std::fs::write(env.directory.join("config.json"),
        r#"{"rateLimit":{"readRequests":2,"writeRequests":2,"loginRequests":2,"globalMultiplier":100}}"#)
        .map_err(|e| e.to_string())?;
    db::open_db()?;
    Ok(env)
}

#[tokio::test]
async fn normal_requests_pass_and_excess_is_429_before_handler() -> Result<(), String> {
    let _env = setup()?;
    for path in ["/api/mesh/security/inbox", "/api/mesh/security/watch", "/api/mesh/security/ack",
        "/api/mesh/risk/decide", "/api/mesh/risk/authorize", "/api/mesh/revoke",
        "/api/mesh/revocations", "/api/mesh/enroll", "/api/mesh/announce", "/api/mesh/assert",
        "/api/account/login", "/api/account/login_code", "/api/collab/login_assertion"] {
        let router = Router::new().route(path, any(|| async { StatusCode::ACCEPTED }))
            .layer(axum::middleware::from_fn(middleware));
        for expected in [StatusCode::ACCEPTED, StatusCode::ACCEPTED, StatusCode::TOO_MANY_REQUESTS] {
            let response = router.clone().oneshot(Request::builder().uri(path)
                .header("authorization", "Bearer device-a").body(Body::empty()).map_err(|e| e.to_string())?)
                .await.map_err(|e| e.to_string())?;
            assert_eq!(response.status(), expected, "{path}");
        }
        if category(path) == Some("login") {
            assert_eq!(check(path, "unknown-peer", Some("Bearer device-b")).err().as_deref(), Some(LIMITED));
        } else {
            assert!(check(path, "unknown-peer", Some("Bearer device-b")).is_ok());
        }
    }
    let leaked: i64 = db::open_db()?.query_row(
        "SELECT COUNT(*) FROM login_attempts WHERE key LIKE '%Bearer%'", [], |r| r.get(0),
    ).map_err(|e| e.to_string())?;
    assert_eq!(leaked, 0);
    Ok(())
}

#[test]
fn windows_reset_and_concurrent_requests_cannot_bypass_budget() -> Result<(), String> {
    let _env = setup()?;
    let path = "/api/mesh/security/ack";
    let threads: Vec<_> = (0..12).map(|_| std::thread::spawn(move || check(path, "peer", Some("key")))).collect();
    let mut accepted = 0;
    for thread in threads {
        if thread.join().map_err(|_| "thread panicked")?.is_ok() { accepted += 1; }
    }
    assert_eq!(accepted, 2);
    db::open_db()?.execute("UPDATE login_attempts SET window_start=0", []).map_err(|e| e.to_string())?;
    assert!(check(path, "peer", Some("key")).is_ok());
    assert!(check("/unrelated", "peer", Some("key")).is_ok());
    Ok(())
}

#[test]
fn invalid_limits_are_rejected_and_old_configs_keep_defaults() -> Result<(), String> {
    for value in [json!(0), json!(-1), json!("20"), json!(10001), json!(1.5), json!(null)] {
        let raw = json!({"rateLimit":{"readRequests":value}}).to_string();
        assert!(interconnect::resolve(Some(&raw), |_| None).is_err());
    }
    assert!(interconnect::resolve(Some(r#"{"rateLimit":{"typo":2}}"#), |_| None).is_err());
    assert_eq!(interconnect::resolve(Some(r#"{"relay":{"fallback":false}}"#), |_| None)?.rate_limit.read_requests, 120);
    Ok(())
}

#[tokio::test]
async fn production_router_installs_rate_gate_before_security_authentication() -> Result<(), String> {
    let _env = setup()?;
    let (sender, _) = tokio::sync::broadcast::channel(4);
    let state = crate::collab::http::CollabState {
        app: crate::host::AppHandle::new(sender),
        auth_token: std::sync::Arc::new(None),
        advertise: std::sync::Arc::new(parking_lot::RwLock::new(Vec::new())),
    };
    let router = crate::collab::http::collab_router(state, false);
    for index in 0..3 {
        let response = router.clone().oneshot(Request::builder().uri("/api/mesh/security/inbox")
            .body(Body::empty()).map_err(|e| e.to_string())?)
            .await.map_err(|e| e.to_string())?;
        if index < 2 { assert_ne!(response.status(), StatusCode::TOO_MANY_REQUESTS); }
        else { assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS); }
    }
    Ok(())
}

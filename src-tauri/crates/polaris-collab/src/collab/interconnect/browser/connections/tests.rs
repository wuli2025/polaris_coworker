use super::*;
use crate::collab::{auth, mesh, interconnect::test_env::TestEnv};

#[test]
fn gateway_connect_authenticates_storage_and_retry_reuses_the_bound_identity() -> Result<(), String> {
    let env = TestEnv::new("browser-connect")?;
    std::env::set_var("POLARIS_ACCOUNT_AUTHORITY", "1");
    std::fs::write(env.directory.join("config.json"), r#"{"browser":{"gatewayUrl":"https://gateway.example"}}"#)
        .map_err(|e| e.to_string())?;
    let (_, uid) = auth::create_authority_account("connect-test", "test-password-123", "Test", "")?;
    let node = "web_connect-test";
    let key = mesh::enroll(&uid, node, "Browser", "web", "1")?;
    let peer = "ab".repeat(32);
    let mut record = Record { uid: uid.clone(), node: node.into(), key, name: "Browser".into(), platform: "web".into(),
        authority_url: None, progress: Value::Null, connections: vec![],
        discovery: json!({"peers":[{"node_id":peer,"name":"NAS","os":"linux","online":true}]}) };
    let mut calls = vec![];
    connect_with(&mut record, |method, url, key, body| {
        calls.push((method.to_string(), url.to_string()));
        assert!(url.starts_with(&format!("https://gateway.example/h/{peer}/api/")));
        if method == "POST" {
            assert!(key.is_none());
            let claims = authority::verify(string(body, "assertion")?)?;
            assert_eq!(claims.uid, uid);
            assert_eq!(claims.node_id.as_deref(), Some(node));
            assert_eq!(body["deviceId"], node);
            Ok(json!({"token":"peer-session"}))
        } else {
            assert_eq!(key, Some("peer-session"));
            Ok(json!({"read":true,"write":false}))
        }
    })?;
    assert_eq!(calls.len(), 2);
    assert_eq!(record.connections[0]["connected"], true);
    assert_eq!(record.connections[0]["mounted"], false, "HTTP access is not an OS drive mount");
    let failure = connect_with(&mut record, |method, _, key, _| {
        assert_eq!(method, "GET");
        assert_eq!(key, Some("peer-session"));
        Err("interconnect_upstream_unavailable".into())
    });
    assert!(failure.is_err());
    assert_eq!(record.connections[0]["state"], "failed");
    connect_with(&mut record, |method, _, key, _| {
        assert_eq!(method, "GET");
        assert_eq!(key, Some("peer-session"));
        Ok(json!({"read":true,"write":true}))
    })?;
    assert_eq!(record.connections[0]["state"], "ready");
    let mut step = 0;
    connect_with(&mut record, |method, _, key, body| {
        step += 1;
        match step {
            1 => { assert_eq!(key, Some("peer-session")); Err("interconnect_upstream_unauthorized".into()) }
            2 => {
                assert_eq!(method, "POST");
                assert_eq!(authority::verify(string(body, "assertion")?)?.node_id.as_deref(), Some(node));
                Ok(json!({"token":"renewed-session"}))
            }
            _ => { assert_eq!(key, Some("renewed-session")); Ok(json!({"read":true})) }
        }
    })?;
    assert_eq!(step, 3);
    assert_eq!(record.connections[0]["token"], "renewed-session");
    Ok(())
}

#[test]
fn retries_are_single_flight_per_session_without_blocking_other_users() {
    let first = session_lock("one");
    let same = session_lock("one");
    let other = session_lock("two");
    let guard = first.try_lock();
    assert!(guard.is_ok());
    assert!(same.try_lock().is_err());
    assert!(other.try_lock().is_ok());
    drop(guard);
    assert!(same.try_lock().is_ok());
}

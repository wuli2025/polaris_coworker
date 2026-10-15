use super::*;
use crate::collab::{auth, authority, db, grants, mesh};
use rusqlite::params;
use serde_json::json;

fn account() -> Result<(), String> {
    db::open_db()?.execute(
        "INSERT INTO users(id,username,pass_hash,role,display_name,created_at,email,uid)
         VALUES(1,'alice','!federated','owner','Alice',1,'alice@example.com','acct_test')", [],
    ).map_err(|e| e.to_string())?;
    db::meta_set("owner_uid", "acct_test")
}

fn pin(key: &str) -> Result<(), String> {
    db::meta_set_many(&[("authority_url", "https://id.example.com"), ("authority_pub", key)])
}

#[test]
fn signed_poll_revokes_warm_sessions_and_survives_restart() -> Result<(), String> {
    let env = test_env::TestEnv::new("propagation")?;
    account()?;
    mesh::enroll("acct_test", "node-a", "Laptop", "linux", "1")?;
    let key = authority::public_key_b64()?;
    let assertion = authority::sign_device_assertion("acct_test", "node-a")?;
    let claims = authority::verify_assertion_with(&assertion, &key)?;
    let initial = revocations::signed("acct_test")?;
    let cloud = env.directory.join("collab.db");
    let member = env.directory.join("member.db");
    std::env::set_var("POLARIS_COLLAB_DB", &member);
    account()?;
    pin(&key)?;
    revocations::accept("acct_test", &initial)?;
    let (_, token) = authority::login_with_assertion(&assertion, "node-a")?;
    assert_eq!(auth::check_session_with_device(&token)?.1, "node-a");
    let stored: String = db::open_db()?.query_row("SELECT device_id FROM sessions WHERE token=?1",
        params![token], |r| r.get(0)).map_err(|e| e.to_string())?;
    assert_eq!(Some(stored), claims.device_id);
    assert!(authority::login_with_assertion(&assertion, "forged-node").is_err());
    std::env::set_var("POLARIS_COLLAB_DB", &cloud);
    mesh::revoke("acct_test", "node-a")?;
    let revoked = revocations::signed("acct_test")?;
    std::env::set_var("POLARIS_COLLAB_DB", &member);
    assert!(auth::check_session(&token).is_ok(), "offline uses last accepted list");
    revocations::accept("acct_test", &revoked)?;
    assert!(auth::check_session(&token).is_err(), "first accepted poll invalidates warm cache");
    assert!(authority::login_with_assertion(&assertion, "node-a").is_err());
    assert!(grants::get("node-a")?.ok_or("missing contract")?.revoked);
    assert!(revocations::accept("acct_test", &initial).is_err(), "rollback is rejected");
    let remaining: i64 = db::open_db()?.query_row("SELECT count(*) FROM sessions WHERE token=?1",
        params![token], |r| r.get(0)).map_err(|e| e.to_string())?;
    assert_eq!(remaining, 0, "session deletion persists on disk");
    Ok(())
}

#[test]
fn tampered_wrong_account_and_unsigned_lists_do_not_replace_cache() -> Result<(), String> {
    let _env = test_env::TestEnv::new("tamper")?;
    let key = authority::public_key_b64()?;
    pin(&key)?;
    let valid = revocations::signed("acct_test")?;
    revocations::accept("acct_test", &valid)?;
    let mut parts: Vec<String> = valid.split('.').map(String::from).collect();
    parts[1].push('A');
    assert!(revocations::accept("acct_test", &parts.join(".")).is_err());
    assert!(revocations::accept("acct_test", "{\"revoked\":[]}").is_err());
    assert!(revocations::accept("other-account", &valid).is_err());
    let stored: String = db::open_db()?.query_row(
        "SELECT document FROM cached_revocation_lists WHERE uid='acct_test'", [], |r| r.get(0),
    ).map_err(|e| e.to_string())?;
    assert_eq!(stored, valid);
    Ok(())
}

#[test]
fn corrupt_cache_denies_high_risk_until_next_security_heartbeat() -> Result<(), String> {
    let env = test_env::TestEnv::new("cache-recovery")?;
    account()?;
    mesh::enroll("acct_test", "node-a", "Laptop", "linux", "1")?;
    mesh::enroll("acct_test", "node-b", "Old device", "linux", "1")?;
    let key = authority::public_key_b64()?;
    let older = revocations::signed("acct_test")?;
    let revoked_device = devices::active("acct_test", "node-b")?.device_id;
    mesh::revoke("acct_test", "node-b")?;
    let valid = revocations::signed("acct_test")?;
    std::env::set_var("POLARIS_COLLAB_DB", env.directory.join("member.db"));
    account()?;
    pin(&key)?;
    revocations::accept("acct_test", &valid)?;
    let payload = json!({"id": 1});
    gate::require("acct_test", "node-a", "delete_account", &payload)?;

    let mut parts: Vec<String> = valid.split('.').map(String::from).collect();
    parts[1].push('A');
    db::open_db()?.execute(
        "UPDATE cached_revocation_lists SET document=?1 WHERE uid='acct_test'",
        params![parts.join(".")],
    ).map_err(|e| e.to_string())?;
    assert!(gate::require("acct_test", "node-a", "delete_account", &payload).is_err());
    let stored: String = db::open_db()?.query_row(
        "SELECT document FROM cached_revocation_lists WHERE uid='acct_test'", [], |r| r.get(0),
    ).map_err(|e| e.to_string())?;
    assert!(stored.is_empty(), "damaged signed payload must be cleared");
    assert!(revocations::require_fresh("acct_test").is_err());
    assert!(revocations::is_revoked("acct_test", &revoked_device)?);
    assert!(sync::poll_account_with("acct_test", || Ok(json!({"document": older}))).is_err());
    assert!(sync::poll_account_with("acct_test", || Err("offline".into())).is_err());
    assert!(gate::require("acct_test", "node-a", "delete_account", &payload).is_err());

    // Run the next heartbeat's account-refresh path with an injected authority response.
    assert!(revocations::accounts()?.iter().any(|uid| uid == "acct_test"));
    sync::poll_account_with("acct_test", || Ok(json!({"document": valid})))?;
    revocations::require_fresh("acct_test")?;
    gate::require("acct_test", "node-a", "delete_account", &payload)?;
    assert!(revocations::is_revoked("acct_test", &revoked_device)?);
    Ok(())
}

#[test]
fn offline_session_works_but_stale_high_risk_requires_online_confirmation() -> Result<(), String> {
    let _env = test_env::TestEnv::new("offline")?;
    account()?;
    mesh::enroll("acct_test", "node-a", "Laptop", "linux", "1")?;
    let key = authority::public_key_b64()?;
    pin(&key)?;
    let assertion = authority::sign_device_assertion("acct_test", "node-a")?;
    let (_, token) = authority::login_with_assertion(&assertion, "node-a")?;
    let stale = authority::sign_document("PR1", &revocations::Snapshot {
        uid: "acct_test".into(), version: 0, issued_at: db::now() - 601, revoked: vec![], devices: vec![], first_device: None,
    })?;
    revocations::accept("acct_test", &stale)?;
    assert!(auth::check_session(&token).is_ok());
    let error = gate::session(&token, "delete_account", &json!({"id":1}))
        .err().ok_or("stale high risk was accepted")?;
    assert!(error.contains("需要联网确认"));
    assert!(grants::fs_gate("node-a", true).is_ok());
    Ok(())
}

#[test]
fn reenrollment_never_restores_a_revoked_device_generation() -> Result<(), String> {
    let _env = test_env::TestEnv::new("generations")?;
    account()?;
    let old_key = mesh::enroll("acct_test", "node-a", "Laptop", "linux", "1")?;
    let old = devices::active("acct_test", "node-a")?;
    mesh::revoke("acct_test", "node-a")?;
    let new_key = mesh::enroll("acct_test", "node-a", "Laptop", "linux", "1")?;
    let new = devices::active("acct_test", "node-a")?;
    assert_ne!(old.device_id, new.device_id);
    assert!(new.sequence > old.sequence);
    assert!(revocations::is_revoked("acct_test", &old.device_id)?);
    assert!(!revocations::is_revoked("acct_test", &new.device_id)?);
    assert!(mesh::resolve_key(&old_key).is_err());
    assert!(mesh::resolve_key(&new_key).is_ok());
    let raw: String = db::open_db()?.query_row("SELECT key_hash FROM mesh_nodes WHERE node_id='node-a'",
        [], |r| r.get(0)).map_err(|e| e.to_string())?;
    assert_ne!(raw, new_key);
    Ok(())
}

#[test]
fn enrollment_notifies_all_preexisting_devices_once() -> Result<(), String> {
    let _env = test_env::TestEnv::new("notifications")?;
    mesh::enroll("acct_test", "node-a", "First", "linux", "1")?;
    mesh::enroll("acct_test", "node-b", "Second", "windows", "1")?;
    mesh::enroll("acct_test", "node-c", "Third", "macos", "1")?;
    let third = devices::active("acct_test", "node-c")?;
    for node in ["node-a", "node-b"] {
        let inbox = notifications::inbox("acct_test", node)?;
        let event = inbox.iter().find(|event| event["deviceId"] == third.device_id).ok_or("missing notification")?;
        assert_eq!(event["name"], "Third");
        assert_eq!(event["platform"], "macos");
        assert!(event["time"].as_i64().is_some_and(|time| time > 0));
        notifications::acknowledge("acct_test", node, &third.device_id)?;
    }
    mesh::enroll("acct_test", "node-c", "Third", "macos", "2")?;
    assert!(notifications::inbox("acct_test", "node-c")?.is_empty());
    assert!(notifications::inbox("acct_test", "node-b")?.is_empty(), "ack survives re-login");
    Ok(())
}

#[test]
fn policy_b_waits_for_existing_device_then_consumes_approval_once() -> Result<(), String> {
    let _env = test_env::TestEnv::new("approvals")?;
    mesh::enroll("acct_test", "node-a", "First", "linux", "1")?;
    let digest = risk::digest("delete_account", &json!({"uid":"acct_test"}));
    risk::authorize("acct_test", "node-a", "node-a", "delete_account", &digest, "nonce")?;
    mesh::enroll("acct_test", "node-b", "Second", "linux", "1")?;
    grants::auto_grant_self("node-b", "acct_test", "Second", false)?;
    assert!(grants::fs_gate("node-b", true).is_ok(), "new device can write immediately");
    assert!(risk::authorize("acct_test", "node-b", "node-a", "delete_account", &digest, "nonce")
        .err().is_some_and(|e| e.starts_with("approval_pending:")));
    let pending = risk::pending("acct_test", "node-a")?;
    let id = pending.first().and_then(|p| p["id"].as_str()).ok_or("missing approval")?;
    assert!(risk::decide("acct_test", "node-b", id, true).is_err(), "self approval denied");
    mesh::enroll("acct_test", "node-c", "Third", "linux", "1")?;
    assert!(risk::decide("acct_test", "node-c", id, true).is_err(), "newer device cannot approve");
    risk::decide("acct_test", "node-a", id, true)?;
    risk::authorize("acct_test", "node-b", "node-a", "delete_account", &digest, "nonce")?;
    assert!(risk::authorize("acct_test", "node-b", "node-a", "delete_account", &digest, "nonce").is_err());
    Ok(())
}

#[test]
fn timeout_and_revoked_approver_cannot_authorize() -> Result<(), String> {
    let _env = test_env::TestEnv::new("timeout")?;
    mesh::enroll("acct_test", "node-a", "First", "linux", "1")?;
    mesh::enroll("acct_test", "node-b", "Second", "linux", "1")?;
    let digest = risk::digest("delete_account", &json!({}));
    assert!(risk::authorize("acct_test", "node-b", "node-b", "delete_account", &digest, "n").is_err());
    let list = risk::pending("acct_test", "node-a")?;
    let id = list.first().and_then(|p| p["id"].as_str()).ok_or("missing approval")?;
    db::open_db()?.execute("UPDATE mesh_approvals SET expires_at=0 WHERE id=?1", params![id])
        .map_err(|e| e.to_string())?;
    assert!(risk::decide("acct_test", "node-a", id, true).is_err());
    assert!(risk::authorize("acct_test", "node-b", "node-b", "delete_account", &digest, "n").is_err());
    mesh::enroll("acct_test", "node-c", "Third", "linux", "1")?;
    let next_digest = risk::digest("delete_account", &json!({"retry":true}));
    assert!(risk::authorize("acct_test", "node-b", "node-b", "delete_account", &next_digest, "n").is_err());
    let list = risk::pending("acct_test", "node-a")?;
    let id = list.first().and_then(|p| p["id"].as_str()).ok_or("missing new request")?;
    risk::decide("acct_test", "node-a", id, true)?;
    mesh::revoke("acct_test", "node-a")?;
    assert!(risk::authorize("acct_test", "node-b", "node-b", "delete_account", &next_digest, "n").is_err());
    Ok(())
}

#[test]
fn legacy_assertions_still_login_and_incomplete_bindings_fail() -> Result<(), String> {
    let _env = test_env::TestEnv::new("legacy")?;
    account()?;
    let key = authority::public_key_b64()?;
    pin(&key)?;
    let assertion = authority::sign_assertion("acct_test", "alice", "alice@example.com", "Alice")?;
    let (_, token) = authority::login_with_assertion(&assertion, "legacy-node")?;
    assert_eq!(auth::check_session_with_device(&token)?.1, "legacy-node");
    let mut claims = authority::verify_assertion_with(&assertion, &key)?;
    claims.device_id = Some("missing-node".into());
    assert!(binding::node(&claims, "legacy-node").is_err());
    Ok(())
}

#[test]
fn security_configuration_validates_types_and_bounds() {
    for raw in [r#"{"security":{"revocationPollSecs":0}}"#,
        r#"{"security":{"revocationMaxAgeSecs":"600"}}"#,
        r#"{"security":{"newDeviceWindowHours":-1}}"#,
        r#"{"security":{"approvalTimeoutSecs":3601}}"#] {
        assert!(resolve(Some(raw), |_| None).is_err(), "accepted {raw}");
    }
}

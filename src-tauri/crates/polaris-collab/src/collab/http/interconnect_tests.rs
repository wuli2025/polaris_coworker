use super::*;
use crate::collab::{auth, authority, interconnect::test_env::TestEnv};

async fn body(response: Response) -> Result<Value, String> {
    assert!(response.status().is_success());
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await
        .map_err(|e| e.to_string())?;
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}

#[tokio::test]
async fn login_enrollment_and_heartbeat_advertise_same_relay_list() -> Result<(), String> {
    let _env = TestEnv::new("wire-relays")?;
    std::env::set_var("POLARIS_ACCOUNT_AUTHORITY", "1");
    std::env::set_var("POLARIS_RELAYS", "https://r1.example.com,https://r2.example.com");
    let (_, uid) = auth::create_authority_account("relayuser", "test-password-123", "Relay", "")?;
    let response = body(account_login(Json(json!({
        "username": "relayuser", "password": "test-password-123",
    }))).await).await?;
    assert_eq!(response["relays"], json!(["https://r1.example.com", "https://r2.example.com"]));
    let assertion = response["assertion"].as_str().ok_or("missing assertion")?;
    let claims = authority::verify_assertion_with(assertion, &authority::public_key_b64()?)?;
    assert_eq!(claims.exp - claims.iat, 300, "relay rollout must preserve assertion TTL");
    assert_eq!(claims.uid, uid);
    let enrolled = body(mesh_enroll(Json(json!({
        "assertion": assertion, "nodeId": "relay-test-device", "name": "Device", "os": "linux",
    }))).await).await?;
    assert_eq!(enrolled["relays"], response["relays"]);
    let key = enrolled["meshKey"].as_str().ok_or("missing mesh key")?;
    let mut headers = HeaderMap::new();
    headers.insert("authorization", format!("Bearer {key}").parse().map_err(|e| format!("{e}"))?);
    let announced = body(mesh_announce(headers, Json(json!({}))).await).await?;
    assert_eq!(announced["relays"], response["relays"]);
    assert_eq!(announced["uid"], uid);
    Ok(())
}

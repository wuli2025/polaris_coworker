use super::*;

#[test]
fn placeholders_are_local_and_defaults_ship_with_schema() -> Result<(), String> {
    let config = resolve(None, |_| None)?;
    assert_eq!(config.authority.url, None);
    assert!(config.authority.public_key_fingerprints.is_empty());
    assert!(config.relay.fallback);
    let schema: serde_json::Value = serde_json::from_str(SCHEMA).map_err(|e| e.to_string())?;
    assert_eq!(schema["properties"]["relay"]["properties"]["fallback"]["default"], true);
    Ok(())
}

#[test]
fn precedence_is_environment_then_user_then_bundle() -> Result<(), String> {
    let config = resolve(Some(r#"{
        "authority":{"url":"https://user.example"},
        "relay":{"urls":["https://relay.example"],"fallback":false}
    }"#), |key| match key {
        "POLARIS_ACCOUNT_AUTHORITY_URL" => Some("https://env.example/".into()),
        "POLARIS_RELAY_FALLBACK" => Some("true".into()),
        _ => None,
    })?;
    assert_eq!(config.authority.url.as_deref(), Some("https://env.example"));
    assert_eq!(config.relay.urls, ["https://relay.example"]);
    assert!(config.relay.fallback);
    let cleared = resolve(Some(r#"{"authority":{"url":"https://user.example"}}"#), |key| {
        (key == "POLARIS_ACCOUNT_AUTHORITY_URL").then(String::new)
    })?;
    assert!(cleared.authority.url.is_none());
    Ok(())
}

#[test]
fn reject_bad_types_unknown_fields_invalid_urls_and_short_fingerprints() {
    for raw in [
        "[]", r#"{"relay":false}"#, r#"{"relay":{"fallback":"false"}}"#,
        r#"{"relay":{"fallbak":false}}"#, r#"{"authority":{"url":42}}"#,
        r#"{"authority":{"url":"https://"}}"#,
        r#"{"authority":{"url":"https://user:secret@example.com"}}"#,
        r#"{"authority":{"publicKeyFingerprints":["abcdef0123456789"]}}"#,
    ] {
        assert!(resolve(Some(raw), |_| None).is_err(), "accepted {raw}");
    }
    assert!(resolve(None, |key| {
        (key == "POLARIS_RELAY_FALLBACK").then(|| "yes".into())
    }).is_err());
}

#[test]
fn preconfigured_key_mismatch_is_rejected_and_rotation_accepts_either() -> Result<(), String> {
    let key = |seed| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
        ed25519_dalek::SigningKey::from_bytes(&[seed; 32]).verifying_key().to_bytes()
    );
    let first = key(1);
    let second = key(2);
    let attacker = key(3);
    let mut config = resolve(None, |_| None)?;
    assert!(config.verify_key(&first).is_err(), "must not silently establish TOFU");
    config.authority.public_key_fingerprints = vec![fingerprint(&first)?, fingerprint(&second)?];
    config.verify_key(&first)?;
    config.verify_key(&second)?;
    let error = config.verify_key(&attacker).err().ok_or("accepted attacker key")?;
    assert!(error.contains("指纹不符"));
    assert!(config.verify_key("bad-base64").is_err());
    Ok(())
}

#[test]
fn cached_pin_is_checked_again_when_deployment_fingerprints_change() -> Result<(), String> {
    use super::super::{authority, db};
    let _env = test_env::TestEnv::new("cached-pin")?;
    assert_eq!(authority::mode(), authority::Mode::Local);
    let public = authority::public_key_b64()?;
    let assertion = authority::sign_assertion("acct_test", "alice", "a@example.com", "Alice")?;
    db::meta_set_many(&[("authority_url", "https://id.example.com"), ("authority_pub", &public)])?;
    // Existing historical pins remain usable with the unconfigured bundle.
    authority::verify_assertion(&assertion)?;
    std::env::set_var("POLARIS_ACCOUNT_AUTHORITY_PUBLIC_KEY_FINGERPRINTS", "0".repeat(64));
    assert!(authority::verify_assertion(&assertion).is_err());
    assert!(authority::pin_explicit("https://id.example.com").is_err());
    std::env::set_var("POLARIS_ACCOUNT_AUTHORITY_PUBLIC_KEY_FINGERPRINTS", fingerprint(&public)?);
    authority::verify_assertion(&assertion)?;
    authority::pin_explicit("https://id.example.com")?;
    Ok(())
}

#[test]
fn first_contact_without_fingerprints_is_rejected_before_network() -> Result<(), String> {
    let _env = test_env::TestEnv::new("first-contact")?;
    let error = super::super::authority::pin_explicit("https://id.example.com")
        .err().ok_or("unexpected first-contact trust")?;
    assert!(error.contains("未配置权威公钥指纹"));
    assert!(super::super::authority::pinned().is_none());
    Ok(())
}

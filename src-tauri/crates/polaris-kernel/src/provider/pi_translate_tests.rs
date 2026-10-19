use super::*;
fn view(protocol: &str, cfg: Value) -> ProviderView {
    ProviderView { id: "gateway".into(), name: "Gateway".into(), note: String::new(),
        base_url: "https://example.test/v1".into(), token_field: "ANTHROPIC_AUTH_TOKEN".into(),
        category: "custom".into(), website_url: String::new(), color: String::new(),
        kind: "custom".into(), protocol: protocol.into(), is_preset: false, has_key: true,
        auth_token: "never-in-config".into(), settings_config: cfg }
}
#[test]
fn protocols_priority_and_missing_metadata() -> Result<(), String> {
    let defaults: PiSettings = serde_json::from_str(include_str!("pi-default.json")).map_err(|e| e.to_string())?;
    for (protocol, api) in [("", "anthropic-messages"), ("openai", "openai-completions"), ("responses", "openai-responses")] {
        let v = view(protocol, json!({"models":[{"id":"explicit","contextWindow":64000}],
            "env":{"ANTHROPIC_MODEL":"env-model","ANTHROPIC_AUTH_TOKEN":"never-in-config"}}));
        let config = translate(&v, &[], &defaults)?;
        assert_eq!(config["api"], api);
        assert_eq!(config["authHeader"], protocol.is_empty());
        assert_eq!(config["models"][0]["id"], "explicit");
        assert_eq!(config["models"][0]["contextWindow"], 64000);
        assert!(!config.to_string().contains("never-in-config"));
    }
    let v = view("openai", json!({"env":{"ANTHROPIC_MODEL":"env-model","OPENAI_MODEL":"env-model"}}));
    let config = translate(&v, &[json!({"provider":"gateway","id":"builtin"})], &defaults)?;
    assert_eq!(config["models"].as_array().map(Vec::len), Some(1));
    assert_eq!(config["models"][0]["id"], "env-model");
    let v = view("openai", json!({}));
    assert_eq!(translate(&v, &[json!({"provider":"gateway","id":"builtin"})], &defaults)?["models"][0]["id"], "builtin");
    assert!(translate(&v, &[], &defaults).is_err());
    assert!(translate(&view("unknown", json!({"models":["m"]})), &[], &defaults).is_err());
    assert!(translate(&view("openai", json!({"models":[{"id":"m","maxTokens":0}]})), &[], &defaults).is_err());
    Ok(())
}
#[test]
fn pi_error_categories_drive_the_existing_sliding_window() {
    let tracker = polaris_failover::FailoverTracker::new();
    assert!(!super::super::pi_failure_counts("MODEL_CONTEXT_OVERFLOW"));
    assert!(!super::super::pi_failure_counts("TURN_CANCELLED"));
    assert!(!tracker.record_failure("p"));
    assert!(!tracker.record_failure("p"));
    tracker.record_success("p");
    for (i, code) in ["MODEL_AUTH", "MODEL_RATE_LIMIT", "MODEL_NETWORK"].iter().enumerate() {
        assert!(super::super::pi_failure_counts(code));
        assert_eq!(tracker.record_failure("p"), i == 2);
    }
}

#[test]
fn codex_default_is_astra_and_explicit_model_wins() {
    let defaults = super::super::pi_config::load().unwrap();
    let mut v = view("chatgpt", json!({}));
    v.id = "codex".into();
    let translated = translate(&v, &[], &defaults).unwrap();
    assert_eq!(translated["models"][0]["id"], store::bundled::CODEX_MODEL);
    v.settings_config = json!({"models":["user-model"]});
    assert_eq!(translate(&v, &[], &defaults).unwrap()["models"][0]["id"], "user-model");
}

//! Saving must work with no backend, identity, model cache or network.
use polaris_kernel::provider;
use serde_json::json;
use std::fs;

#[test]
fn save_without_backend_then_validate_cached_metadata() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::temp_dir().join(format!("p7-offline-save-{}", std::process::id()));
    fs::create_dir_all(&dir)?;
    let config = dir.join("agent.json");
    fs::write(&config, serde_json::to_vec(&json!({"agentd":{
        "isolation":"per-account","node":"/does-not-exist","entry":"/does-not-exist"
    }}))?)?;
    std::env::set_var("POLARIS_AGENT_CONFIG", &config);
    std::env::set_var("POLARIS_PROVIDER_STORE", dir.join("providers.json"));
    let input = |models: Option<serde_json::Value>| {
        let mut settings = json!({});
        if let Some(models) = models { settings["models"] = models; }
        serde_json::from_value(json!({"id":"codex","name":"Offline subscription","settingsConfig":settings}))
    };
    // No SDK catalog yet: absence of inferred model metadata is not a save error.
    assert_eq!(provider::provider_save(input(None)?)?, "codex");
    assert_eq!(provider::provider_save(input(Some(json!(["offline-model"])))?)?, "codex");
    let before = serde_json::to_value(provider::store::pi_views())?;
    let error = provider::provider_save(input(Some(json!([{"id":"bad","maxTokens":0}])))?)
        .err().ok_or("invalid model accepted")?;
    assert!(error.contains("/settingsConfig/models/0/maxTokens"));
    assert_eq!(serde_json::to_value(provider::store::pi_views())?, before);
    // Only cached metadata supplies the tiny context: structural defaults alone
    // would accept this output limit, but the cached translation must reject it.
    provider::pi_translate::cache_catalog(&json!({"models":[{
        "provider":"openai-codex","id":"cached","name":"cached","contextWindow":100,
        "maxTokens":50,"reasoning":false,"input":["text"],
        "cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0}
    }]}));
    let error = provider::provider_save(input(Some(json!([{"id":"cached","maxTokens":101}])))?)
        .err().ok_or("cached context ignored")?;
    assert!(error.contains("/settingsConfig/models/0/maxTokens"));
    assert_eq!(serde_json::to_value(provider::store::pi_views())?, before);
    assert_eq!(provider::provider_save(input(Some(json!(["cached"])))?)?, "codex");
    // The full switch still requires a usable backend and authenticated isolation.
    assert!(provider::provider_switch("codex".into()).is_err());
    fs::remove_dir_all(dir)?;
    Ok(())
}

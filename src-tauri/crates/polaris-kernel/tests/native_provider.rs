//! Real Rust command -> agentd -> Pi registration, isolated from user credentials.
use polaris_kernel::{chat::backend, provider};
use serde_json::json;
use std::{fs, path::Path};

#[test]
fn save_and_switch_native_and_reject_invalid_save_with_paths() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::temp_dir().join(format!("p7-provider-{}", std::process::id()));
    fs::create_dir_all(&dir)?;
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let config = dir.join("agent.json");
    let daemon_config = dir.join("daemon.json");
    fs::write(&daemon_config, serde_json::to_vec(&json!({"sessionDir":dir.join("sessions"),"usageDir":dir.join("usage")}))?)?;
    fs::write(&config, serde_json::to_vec(&json!({"agent":{"backend":"pi"},"agentd":{
        "entry":repo.join("agentd/src/main.ts"),"args":["--config",daemon_config],
        "nodeArgs":["--import",repo.join("agentd/node_modules/tsx/dist/loader.mjs")],
        "isolation":"shared",
        "sessionMap":dir.join("session-map"),"requestTimeoutMs":30000
    }}))?)?;
    std::env::set_var("POLARIS_AGENT_CONFIG", &config);
    std::env::set_var("POLARIS_PROVIDER_STORE", dir.join("providers.json"));
    let input = |models| serde_json::from_value(json!({"id":"codex","name":"Drill subscription","settingsConfig":{"models":models}}));
    let backend = backend::selected()?;
    for name in ["gpt-6-astra", "polaris-out-of-catalog-test"] {
        assert_eq!(provider::provider_save(input(json!([name]))?)?, "codex");
        assert_eq!(provider::provider_switch("codex".into())?, "codex");
        let models = backend.models()?;
        let model = models["models"].as_array().ok_or("missing catalog")?.iter()
            .find(|m| m["provider"] == "openai-codex" && m["id"] == name).ok_or("missing native model")?;
        assert_eq!(model["api"], "openai-codex-responses");
        if name == "polaris-out-of-catalog-test" {
            assert_eq!(model["contextWindow"], 128000);
            assert_eq!(model["maxTokens"], 8192);
            assert_eq!(model["cost"], json!({"input":0,"output":0,"cacheRead":0,"cacheWrite":0}));
        }
    }
    let error = provider::provider_save(input(json!([{"id":"bad","maxTokens":0}]))?)
        .err().ok_or("invalid save accepted")?;
    assert!(error.contains("/settingsConfig/models/0/maxTokens"), "{error}");
    assert_eq!(provider::provider_switch("codex".into())?, "codex", "invalid save must not replace valid config");
    // Exercise the exact WireError -> String return consumed by Tauri and HTTP.
    let pi = backend::pi::PiBackend::new(backend::config::Config::load()?.agentd)?;
    let error = pi.apply_config(Some(json!({"providers":[{"id":"openai-codex","models":[]}]})), None)
        .err().ok_or("invalid wire config accepted")?.to_string();
    assert!(error.contains("INVALID_CONFIG") && error.contains("/providers/0/models"), "{error}");
    use backend::AgentBackend;
    pi.shutdown();
    backend.shutdown();
    std::env::remove_var("POLARIS_AGENT_CONFIG");
    fs::remove_dir_all(dir)?;
    Ok(())
}

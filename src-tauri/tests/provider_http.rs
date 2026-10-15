#![cfg(all(feature = "server", not(feature = "desktop")))]
//! Exercise the public HTTP error envelope and the shared Tauri command return.
use axum::{body::{Body, to_bytes}, http::{Request, StatusCode}};
use polaris_app_lib::{apihub::{api_router, ApiState}, host::AppHandle};
use polaris_kernel::{provider, chat::backend};
use serde_json::{json, Value};
use std::{fs, sync::Arc, path::Path};
use tower::ServiceExt;

#[tokio::test(flavor = "multi_thread")]
async fn invalid_native_save_preserves_paths_through_http_and_command_return() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::temp_dir().join(format!("p7-http-{}", std::process::id()));
    fs::create_dir_all(&dir)?;
    std::env::set_var("POLARIS_COLLAB_DB", dir.join("collab.db"));
    std::env::set_var("POLARIS_INTERCONNECT_CONFIG", dir.join("interconnect.json"));
    fs::write(dir.join("interconnect.json"), "{}")?;
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let config = dir.join("agent.json");
    fs::write(&config, serde_json::to_vec(&json!({"agent":{"backend":"pi"},"agentd":{
        "entry":repo.join("src-tauri/crates/polaris-kernel/tests/fixtures/fake-agentd.mjs"),
        "isolation":"shared","sessionMap":dir.join("sessions"),
        "expertSettings":dir.join("experts.json"),"requestTimeoutMs":30000
    }}))?)?;
    std::env::set_var("POLARIS_AGENT_CONFIG", &config);
    std::env::set_var("POLARIS_PROVIDER_STORE", dir.join("providers.json"));
    let (tx, _) = tokio::sync::broadcast::channel(32);
    let router = api_router(ApiState { app: AppHandle::new(tx.clone()), tx,
        auth_token: Arc::new(Some("p7-http-test-token".into())), open_no_auth: false });
    // Saving validates locally; switching still preserves SDK WireError.paths.
    let input = json!({"id":"codex","name":"bad","settingsConfig":{"models":[{"id":"invalid","maxTokens":0}]}});
    let response = router.clone().oneshot(Request::post("/api/invoke")
        .header("content-type", "application/json").header("authorization", "Bearer p7-http-test-token")
        .body(Body::from(json!({"cmd":"provider_save","args":{"input":input}}).to_string()))?).await?;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body: Value = serde_json::from_slice(&to_bytes(response.into_body(), 65536).await?)?;
    let expected = provider::provider_save(serde_json::from_value(input)?).err().ok_or("invalid save accepted")?;
    assert_eq!(body["error"], expected);
    assert!(expected.contains("/settingsConfig/models/0/maxTokens"), "{expected}");
    provider::provider_save(serde_json::from_value(json!({"id":"codex","name":"SDK rejected",
        "settingsConfig":{"models":["invalid-wire-config"]}}))?)?;
    let response = router.clone().oneshot(Request::post("/api/invoke")
        .header("content-type", "application/json").header("authorization", "Bearer p7-http-test-token")
        .body(Body::from(json!({"cmd":"provider_switch","args":{"id":"codex"}}).to_string()))?).await?;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body: Value = serde_json::from_slice(&to_bytes(response.into_body(), 65536).await?)?;
    assert!(body["error"].as_str().is_some_and(|error| error.contains("/providers/0/models/0/name")));
    for (cmd, args) in [
        ("provider_save", json!({"input":{"id":"codex","name":"Drill subscription","settingsConfig":{"models":["gpt-6-astra"]}}})),
        ("provider_switch", json!({"id":"codex"})),
    ] {
        let response = router.clone().oneshot(Request::post("/api/invoke")
            .header("content-type", "application/json").header("authorization", "Bearer p7-http-test-token")
            .body(Body::from(json!({"cmd":cmd,"args":args}).to_string()))?).await?;
        let status = response.status();
        let body: Value = serde_json::from_slice(&to_bytes(response.into_body(), 65536).await?)?;
        assert_eq!(status, StatusCode::OK, "{cmd}: {body}");
        assert_eq!(body, "codex");
    }
    backend::selected()?.shutdown();
    fs::remove_dir_all(dir)?;
    Ok(())
}

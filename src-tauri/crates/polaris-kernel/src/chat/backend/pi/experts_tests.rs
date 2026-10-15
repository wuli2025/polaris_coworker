use super::*;
use crate::chat::backend::AgentBackend;

#[test]
fn prompt_limit_defaults_and_rejects_invalid_values() -> anyhow::Result<()> {
    assert_eq!(parse_prompt_limit(None)?, 32768);
    assert_eq!(parse_prompt_limit(Some("65536"))?, 65536);
    for value in ["", "0", "-1", "1.5", "true", "unlimited", "999999999999999999999999999999"] {
        let error = parse_prompt_limit(Some(value)).err()
            .ok_or_else(|| anyhow::anyhow!("accepted invalid limit"))?;
        assert!(error.to_string().starts_with("ExpertPromptLimitInvalid:"));
    }
    Ok(())
}

#[test]
fn prompt_limits_count_bytes_and_normalize_only_trailing_whitespace() -> anyhow::Result<()> {
    let config = |prompt: String| ExpertConfig { enabled: true, prompt };
    assert_eq!(config("x".repeat(32768)).normalized(32768)?.prompt.len(), 32768);
    for prompt in ["x".repeat(32769), "界".repeat(10923), " ".repeat(32769)] {
        let error = config(prompt).normalized(32768).err()
            .ok_or_else(|| anyhow::anyhow!("accepted oversized prompt"))?;
        assert!(error.to_string().starts_with("ExpertPromptTooLong:"));
    }
    assert_eq!(config("界".into()).normalized(3)?.prompt, "界");
    assert!(config("界".into()).normalized(2).is_err());
    assert_eq!(config("  keep\ninternal \n\t\u{2003}".into()).normalized(100)?.prompt, "  keep\ninternal");
    assert_eq!(config("expert\n\t".into()).normalized(100)?, config("expert".into()).normalized(100)?);
    Ok(())
}

#[test]
fn persisted_prompts_use_configured_limit_on_load() -> anyhow::Result<()> {
    let path = super::super::tests::config().session_map.with_extension("prompt-limit.json");
    fs::write(&path, r#"{"enabled":true,"prompt":"界界  "}"#)?;
    assert!(load_with_limit(&path, 7).is_err_and(|e| e.to_string().contains("ExpertPromptTooLong")));
    assert_eq!(load_with_limit(&path, 8)?.map(|c| c.prompt), Some("界界".into()));
    fs::remove_file(path)?;
    Ok(())
}

#[test]
fn trailing_whitespace_does_not_reapply_equivalent_persisted_config() -> anyhow::Result<()> {
    let mut config = super::super::tests::config();
    let root = config.session_map.with_extension("experts-whitespace");
    fs::create_dir_all(&root)?;
    config.expert_settings = root.join("settings.json");
    let record = root.join("requests.jsonl");
    config.args = vec!["--record".into(), record.to_string_lossy().into_owned()];
    fs::write(&config.expert_settings, r#"{"enabled":false,"prompt":"deployment-prompt\n\t "}"#)?;
    let backend = PiBackend::new(config)?;
    assert!(!backend.experts_enabled(None)?);
    let requests = fs::read_to_string(record)?;
    for line in requests.lines() {
        let request: serde_json::Value = serde_json::from_str(line)?;
        assert_ne!(request["method"], "config.apply");
    }
    backend.shutdown();
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn oversized_daemon_prompt_is_rejected_before_setter_persistence() -> anyhow::Result<()> {
    let mut config = super::super::tests::config();
    config.max_line_bytes = 131072;
    config.expert_settings = config.session_map.with_extension("oversized.json");
    let path = config.expert_settings.clone();
    let backend = PiBackend::new(config)?;
    backend.apply_config(Some(serde_json::json!({
        "experts": { "enabled": false, "prompt": "x".repeat(32769) }
    })), None)?;
    assert!(backend.experts_enabled(Some(true)).is_err_and(|e| e.to_string().contains("ExpertPromptTooLong")));
    assert!(!path.exists());
    backend.shutdown();
    Ok(())
}

#[test]
fn setter_delivers_persists_and_restores_after_host_restart() -> anyhow::Result<()> {
    let mut config = super::super::tests::config();
    let root = config.session_map.with_extension("experts-test");
    fs::create_dir_all(&root)?;
    config.expert_settings = root.join("settings.json");
    let backend = PiBackend::new(config.clone())?;
    assert!(!backend.experts_enabled(None)?);
    assert!(backend.experts_enabled(Some(true))?);
    let saved = load(&config.expert_settings)?.ok_or_else(|| anyhow::anyhow!("missing settings"))?;
    assert!(saved.enabled);
    assert_eq!(saved.prompt, "deployment-prompt");
    backend.shutdown();
    let restarted = PiBackend::new(config)?;
    assert!(restarted.experts_enabled(None)?);
    assert!(!restarted.experts_enabled(Some(false))?);
    assert!(!restarted.experts_enabled(None)?);
    restarted.shutdown();
    fs::remove_dir_all(root)?;
    Ok(())
}

#[cfg(unix)]
#[test]
fn failed_persistence_rolls_runtime_back() -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut config = super::super::tests::config();
    let root = config.session_map.with_extension("experts-rollback");
    fs::create_dir_all(&root)?;
    config.expert_settings = root.join("settings.json");
    let backend = PiBackend::new(config)?;
    assert!(!backend.experts_enabled(None)?);
    fs::set_permissions(&root, fs::Permissions::from_mode(0o500))?;
    let result = backend.experts_enabled(Some(true));
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
    assert!(result.is_err_and(|e| e.to_string().contains("ExpertConfigWriteFailed")));
    assert!(!backend.actual_experts()?.enabled);
    backend.shutdown();
    fs::remove_dir(root)?;
    Ok(())
}
#[test]
fn persisted_settings_validate_and_survive_reload() -> anyhow::Result<()> {
    let path = std::env::temp_dir().join(format!("p5d-experts-{}.json", std::process::id()));
    let _ = fs::remove_file(&path);
    assert!(load(&path)?.is_none());
    let config = ExpertConfig { enabled: true, prompt: "configured expert".into() };
    persist(&path, &config)?;
    assert_eq!(load(&path)?, Some(config));
    fs::write(&path, r#"{"enabled":"true","prompt":""}"#)?;
    assert!(load(&path).is_err());
    fs::remove_file(path)?;
    Ok(())
}

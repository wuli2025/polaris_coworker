//! Separate processes keep environment overrides isolated from parallel unit tests.
use polaris_kernel::chat::backend::{config::Config, selected};
#[test]
fn selection_probe() {
    let Ok(expected) = std::env::var("POLARIS_TEST_EXPECT_BACKEND") else { return; };
    let result = selected();
    if expected == "invalid" {
        assert!(result.err().expect("invalid backend accepted").to_string().contains("InvalidBackend"));
    } else {
        assert_eq!(result.expect("backend selection").status().backend, expected);
    }
}
#[test]
fn config_file_and_environment_select_both_backends() {
    let root = std::env::temp_dir().join(format!("backend-config-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("agent.json");
    for (configured, override_value, expected) in [
        ("pi", None, "pi"), ("claude-cli", None, "claude-cli"),
        ("pi", Some("claude-cli"), "claude-cli"), ("claude-cli", Some("pi"), "pi"),
        ("invalid", None, "invalid"), ("pi", Some("invalid"), "invalid"),
    ] {
        std::fs::write(&path, format!(r#"{{"agent":{{"backend":"{configured}"}}}}"#)).unwrap();
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command.args(["--exact", "selection_probe", "--nocapture"])
            .env("POLARIS_AGENT_CONFIG", &path).env("POLARIS_TEST_EXPECT_BACKEND", expected)
            .env_remove("POLARIS_AGENT_BACKEND");
        if let Some(value) = override_value { command.env("POLARIS_AGENT_BACKEND", value); }
        let output = command.output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stdout));
    }
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(Config::default().agent.backend, "pi");
}

#[test]
fn turn_timeout_defaults_types_and_serialization() -> anyhow::Result<()> {
    let defaults = Config::default();
    assert_eq!(defaults.agentd.turn_idle_timeout_ms, 240_000);
    assert!(defaults.agentd.turn_idle_timeout_ms > 180_000);
    assert_eq!(defaults.agentd.turn_max_ms, 0);
    defaults.validate()?;
    let configured: Config = serde_json::from_str(r#"{"agentd":{"turnIdleTimeoutMs":450000,"turnMaxMs":900000}}"#)?;
    configured.validate()?;
    let value = serde_json::to_value(&configured.agentd)?;
    assert_eq!(value["turnIdleTimeoutMs"], 450_000);
    assert_eq!(value["turnMaxMs"], 900_000);
    assert!(value.get("turnTimeoutMs").is_none());
    for field in ["turnIdleTimeoutMs", "turnMaxMs"] {
        for invalid in ["-1", "1.5", "null", "true", "\"300\"", "18446744073709551616"] {
            let json = format!(r#"{{"agentd":{{"{field}":{invalid}}}}}"#);
            assert!(serde_json::from_str::<Config>(&json).is_err(), "accepted {json}");
        }
    }
    let zero: Config = serde_json::from_str(r#"{"agentd":{"turnIdleTimeoutMs":0}}"#)?;
    let error = zero.validate().err().ok_or_else(|| anyhow::anyhow!("accepted zero idle"))?;
    assert!(error.to_string().contains("turnIdleTimeoutMs"));
    Ok(())
}

#[test]
fn removed_turn_timeout_reports_named_migration_error() -> anyhow::Result<()> {
    for old in ["600000", "0", "null", "\"old\"", "{}"] {
        for fields in [String::new(), ",\"turnIdleTimeoutMs\":240000,\"turnMaxMs\":0".into()] {
            let json = format!(r#"{{"agentd":{{"turnTimeoutMs":{old}{fields}}}}}"#);
            let error = serde_json::from_str::<Config>(&json).err()
                .ok_or_else(|| anyhow::anyhow!("accepted removed turnTimeoutMs"))?.to_string();
            for expected in ["InvalidAgentConfig", "turnTimeoutMs", "turnIdleTimeoutMs", "turnMaxMs"] {
                assert!(error.contains(expected), "{error}");
            }
        }
    }
    Ok(())
}

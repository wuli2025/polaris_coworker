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

use super::*;
#[test]
fn credential_report_is_boolean_and_rejects_incomplete_entries() {
    for credential in [serde_json::json!({"type":"api_key","key":"private-key"}),
        serde_json::json!({"type":"oauth","access":"private-token","refresh":"private-refresh"})] {
        assert!(credentials_ready(&serde_json::json!({"credentials":{"p":credential}})));
    }
    for credential in [serde_json::json!({"type":"api_key","key":""}), serde_json::json!({"type":"oauth","access":"a"})] {
        assert!(!credentials_ready(&serde_json::json!({"credentials":{"p":credential}})));
    }
    assert!(!credentials_ready(&serde_json::json!({})));
}
#[test]
fn readiness_requires_every_runtime_component() {
    let ready = AgentRuntimeReport { node_ready: true, entry_exists: true, handshake: true,
        credentials_configured: true, ..Default::default() };
    assert!(ready.ready());
    for i in 0..4 {
        let mut r = ready.clone();
        match i { 0 => r.node_ready = false, 1 => r.entry_exists = false,
            2 => r.handshake = false, _ => r.credentials_configured = false }
        assert!(!r.ready());
    }
    assert!(node_supported("v22.19.0"));
    assert!(!node_supported("v22.18.9"));
    assert!(!node_supported("not-node"));
}
#[test]
fn ping_handles_success_mismatch_failure_and_timeout() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::temp_dir().join(format!("polaris-doctor-ping-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let mut config = Agentd::default();
    config.node = "node".into();
    config.entry = dir.join("probe.mjs");
    config.request_timeout_ms = 1000;
    let result = serde_json::json!({"status":"ok","sessions":0,"version":"fixture", "piVersion":"1.0.4",
        "protocolVersion":crate::chat::backend::pi::protocol::PROTOCOL_VERSION,"usagePersistFailures":0});
    for valid in [true, false] {
        let mut result = result.clone();
        if !valid { result["protocolVersion"] = 0.into(); }
        let frame = serde_json::json!({"type":"response","id":"doctor","ok":true,"result":result});
        std::fs::write(&config.entry, format!("process.stdin.once('data', () => console.log({:?}));", frame.to_string()))?;
        assert_eq!(ping(&config).is_ok(), valid);
        let (_, report) = inspect(&config, true);
        assert_eq!(report.ready(), valid);
        assert_eq!(report.credentials_configured, true);
        assert!(!inspect(&config, false).1.ready());
        let public = serde_json::to_string(&report)?;
        assert!(!public.contains("private-token"));
    }
    let mut wrong_pi = result.clone();
    wrong_pi["piVersion"] = "999.0.0".into();
    let frame = serde_json::json!({"type":"response","id":"doctor","ok":true,"result":wrong_pi});
    std::fs::write(&config.entry, format!("process.stdin.once('data', () => console.log({:?}));", frame.to_string()))?;
    assert_eq!(inspect(&config, true).1.error.as_deref(), Some("AgentdPiVersionMismatch"));
    std::fs::write(&config.entry, "setInterval(() => {}, 1000)")?;
    config.request_timeout_ms = 100;
    assert_eq!(ping(&config).err().as_deref(), Some("AgentdPingTimeout"));
    config.entry = dir.join("missing-entry.mjs");
    assert!(!inspect(&config, true).1.entry_exists);
    config.node = dir.join("absent-node");
    assert!(!inspect(&config, true).1.node_ready);
    assert_eq!(ping(&config).err().as_deref(), Some("AgentdSpawnFailed"));
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
#[test]
fn runtime_paths_support_bundle_development_and_explicit() -> Result<(), Box<dyn std::error::Error>> {
    use crate::chat::backend::config::runtime_paths;
    let root = std::env::temp_dir().join(format!("polaris-path-{}", std::process::id()));
    let node = root.join(if cfg!(windows) { "node/node.exe" } else { "node/bin/node" });
    let entry = root.join("agentd/dist/src/main.js");
    std::fs::create_dir_all(node.parent().ok_or("node parent")?)?;
    std::fs::create_dir_all(entry.parent().ok_or("entry parent")?)?;
    std::fs::write(&node, "")?;
    std::fs::write(&entry, "")?;
    assert_eq!(runtime_paths(None, None, Some(&root), None), (node.clone(), entry));
    assert_eq!(runtime_paths(None, None, None, Some(&root)), ("node".into(), root.join("agentd/dist/src/main.js")));
    assert_eq!(runtime_paths(Some("custom-node".into()), Some("custom-entry".into()), Some(&root), Some(&root)),
        ("custom-node".into(), "custom-entry".into()));
    std::fs::remove_dir_all(root)?;
    Ok(())
}

//! Isolated process: exercises real public commands, selected backend and IPC.
use polaris_kernel::{provider, chat::backend};
use serde_json::{json, Value};
use std::{fs, path::Path, sync::mpsc, thread, time::{Duration, Instant}};

fn commands(path: &Path) -> Vec<Value> {
    match fs::read_to_string(path) {
        Ok(s) => s.lines().filter_map(|line| serde_json::from_str(line).ok()).collect(),
        Err(_) => Vec::new(),
    }
}
fn until(mut condition: impl FnMut() -> bool) -> Result<(), String> {
    let end = Instant::now() + Duration::from_secs(5);
    while !condition() {
        if Instant::now() >= end { return Err("ConditionTimedOut".into()); }
        thread::sleep(Duration::from_millis(5));
    }
    Ok(())
}
#[test]
fn slow_ipc_keeps_queries_available_and_last_selection_wins() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::temp_dir().join(format!("r5-switch-{}", std::process::id()));
    fs::create_dir_all(&dir)?;
    let record = dir.join("requests.jsonl");
    let catalog_gate = dir.join("catalog-ready");
    let apply_gate = dir.join("apply-ready");
    fs::write(&catalog_gate, "")?;
    fs::write(&apply_gate, "")?;
    let config = dir.join("config.json");
    fs::write(&config, serde_json::to_vec(&json!({"agent":{"backend":"pi"},"agentd":{
        "entry":Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-agentd.mjs"),
        "isolation":"shared", "sessionMap":dir.join("sessions"), "requestTimeoutMs":5000,
        "args":["--record",record,"--gate-catalog",catalog_gate,"--gate-apply",apply_gate]
    }}))?)?;
    std::env::set_var("POLARIS_AGENT_CONFIG", &config);
    for id in ["r5-a", "r5-b", "r5-c"] {
        let input = serde_json::from_value(json!({"id":id,"name":id,"protocol":"anthropic","settingsConfig":{
            "env":{"ANTHROPIC_BASE_URL":"https://example.test"},"models":["m"]
        }}))?;
        provider::provider_save(input)?;
    }
    let backend = backend::selected()?;
    backend.models()?;
    assert_eq!(backend::agent_backend_status()?.usage_persist_failures, 7);
    assert_eq!(serde_json::to_value(backend::agent_backend_status()?)?["usagePersistFailures"], 7);
    for (gate, method) in [(&catalog_gate, "models.catalog"), (&apply_gate, "config.apply")] {
        fs::remove_file(gate)?;
        let before = commands(&record).iter().filter(|v| v["method"] == method).count();
        let first = thread::spawn(|| provider::provider_switch("r5-a".into()));
        until(|| commands(&record).iter().filter(|v| v["method"] == method).count() > before)?;
        let second = thread::spawn(|| provider::provider_switch("r5-b".into()));
        until(|| provider::current_provider_id() == "r5-b")?;
        let third = thread::spawn(|| provider::provider_switch("r5-c".into()));
        until(|| provider::current_provider_id() == "r5-c")?;
        // The blocked switch cannot occupy the query thread or the selection lock.
        let (tx, rx) = mpsc::channel();
        let query = thread::spawn(move || { let _ = tx.send(provider::provider_list()); });
        let queried = rx.recv_timeout(Duration::from_millis(500));
        // Always unblock the fixture before propagating test failure.
        fs::write(gate, "")?;
        let list = queried??;
        assert_eq!(list.current_id, "r5-c");
        query.join().map_err(|_| "QueryPanicked")?;
        for worker in [first, second, third] { worker.join().map_err(|_| "SwitchPanicked")??; }
        assert_eq!(provider::current_provider_id(), "r5-c");
        assert_eq!(backend.models()?["models"][0]["provider"], "r5-c");
        let sent = commands(&record);
        let latest = sent.iter().rev().find(|v| v["method"] == "config.apply").ok_or("ApplyMissing")?;
        assert_eq!(latest["params"]["config"]["defaultModel"]["provider"], "r5-c");
        assert!(!sent.iter().any(|v| v["method"] == "config.apply"
            && v["params"]["config"]["defaultModel"]["provider"] == "r5-b"));
    }
    backend.shutdown();
    std::env::remove_var("POLARIS_AGENT_CONFIG");
    fs::remove_dir_all(dir)?;
    Ok(())
}

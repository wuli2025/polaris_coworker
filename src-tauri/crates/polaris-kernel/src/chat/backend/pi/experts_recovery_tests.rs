use super::*;
use crate::chat::backend::{AgentBackend};
use super::super::test_support::{wait_status, age_idle, wait_pings};

fn recovery_config() -> anyhow::Result<super::super::config::Agentd> {
    let mut config = super::super::tests::config();
    let root = config.session_map.with_extension("expert-recovery");
    fs::create_dir_all(&root)?;
    config.expert_settings = root.join("settings.json");
    config.ping_interval_ms = 40;
    // Allow the fixture to be repaired before the existing supervisor circuit opens.
    config.failure_threshold = 1000;
    persist(&config.expert_settings, &ExpertConfig { enabled: true, prompt: "saved prompt".into() })?;
    Ok(config)
}

#[test]
fn unavailable_daemon_does_not_block_backend_and_recovers_without_settings_calls() -> anyhow::Result<()> {
    let mut config = recovery_config()?;
    let fixture = config.entry.clone();
    let root = config.expert_settings.parent().ok_or_else(|| anyhow::anyhow!("missing parent"))?.to_owned();
    config.entry = root.join("initially-missing.mjs");
    let entry = config.entry.clone();
    let backend = PiBackend::new(config)?;
    let status = wait_status(&backend, |s| s.last_error.is_some())?;
    assert!(!status.healthy);
    assert_eq!(status.experts_synced, Some(false));
    assert!(status.expert_sync_error.is_some());
    assert_eq!(serde_json::to_value(&status)?["expertsSynced"], false);
    // No models/get/set call starts recovery: only the supervisor retry and ping do.
    fs::copy(fixture, entry)?;
    let status = wait_status(&backend, |s| s.healthy && s.experts_synced == Some(true))?;
    assert!(status.expert_sync_error.is_none());
    assert!(!backend.models()?["models"].as_array().is_none_or(Vec::is_empty));
    assert_eq!(backend.actual_experts()?, ExpertConfig { enabled: true, prompt: "saved prompt".into() });
    backend.shutdown();
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn invalid_saved_config_keeps_daemon_available_and_retries_on_ping() -> anyhow::Result<()> {
    let config = recovery_config()?;
    let path = config.expert_settings.clone();
    fs::write(&path, r#"{"enabled":"invalid","prompt":"saved prompt"}"#)?;
    let backend = PiBackend::new(config)?;
    let status = wait_status(&backend, |s| s.healthy && s.expert_sync_error.is_some())?;
    assert_eq!(status.experts_synced, Some(false));
    assert!(status.expert_sync_error.is_some_and(|e| e.contains("ExpertConfigInvalid")));
    assert!(backend.models().is_ok());
    assert!(backend.experts_enabled(Some(false)).is_err_and(|e| e.to_string().contains("ExpertConfigUnsynced")));
    persist(&path, &ExpertConfig { enabled: true, prompt: "repaired".into() })?;
    wait_status(&backend, |s| s.healthy && s.experts_synced == Some(true))?;
    assert_eq!(backend.actual_experts()?.prompt, "repaired");
    backend.shutdown();
    fs::remove_dir_all(path.parent().ok_or_else(|| anyhow::anyhow!("missing parent"))?)?;
    Ok(())
}

#[test]
fn apply_rejection_and_verification_mismatch_retry_without_poisoning_health() -> anyhow::Result<()> {
    for mode in ["reject", "mismatch"] {
        let mut config = recovery_config()?;
        let root = config.expert_settings.parent().ok_or_else(|| anyhow::anyhow!("missing parent"))?.to_owned();
        let gate = root.join("allow-apply");
        config.node_args = vec!["-e".into(), r#"
const fs = require('node:fs');
const readline = require('node:readline');
const [gate, mode] = process.argv.slice(-2);
let experts = {enabled: false, prompt: 'deployment-prompt'};
readline.createInterface({input: process.stdin}).on('line', line => {
  const {id, method, params} = JSON.parse(line);
  const send = value => process.stdout.write(JSON.stringify({type:'response', id, ...value}) + '\n');
  if (method === 'ping') return send({ok:true, result:{status:'ok', sessions:0,
    version:'test', piVersion:'1', protocolVersion:6, usagePersistFailures:0, experts}});
  if (method === 'config.apply') {
    if (!fs.existsSync(gate) && mode === 'reject') {
      return send({ok:false, error:{code:'APPLY_REJECTED', message:'fixture rejection'}});
    }
    if (fs.existsSync(gate)) experts = params.config.experts;
  }
  send({ok:true, result:{models:[]}});
});
"#.into()];
        config.args = vec![gate.to_string_lossy().into_owned(), mode.into()];
        let backend = PiBackend::new(config)?;
        let expected = if mode == "reject" { "APPLY_REJECTED" } else { "ExpertConfigApplyMismatch" };
        let status = wait_status(&backend, |s| s.expert_sync_error.as_ref().is_some_and(|e| e.contains(expected)))?;
        assert!(status.healthy);
        assert_eq!(status.experts_synced, Some(false));
        backend.models()?;
        fs::write(gate, "ready")?;
        wait_status(&backend, |s| s.experts_synced == Some(true))?;
        assert!(backend.actual_experts()?.enabled);
        backend.shutdown();
        fs::remove_dir_all(root)?;
    }
    Ok(())
}

#[test]
fn idle_daemon_reopen_restores_experts_before_releasing_requests() -> anyhow::Result<()> {
    let config = recovery_config()?;
    let path = config.expert_settings.clone();
    let backend = PiBackend::new(config)?;
    wait_status(&backend, |s| s.experts_synced == Some(true))?;
    age_idle(&backend)?;
    let idle = wait_status(&backend, |s| s.pid.is_none())?;
    assert_eq!(idle.experts_synced, Some(false));
    backend.models()?;
    assert_eq!(backend.status().experts_synced, Some(true));
    assert_eq!(backend.actual_experts()?.prompt, "saved prompt");
    backend.shutdown();
    fs::remove_dir_all(path.parent().ok_or_else(|| anyhow::anyhow!("missing parent"))?)?;
    Ok(())
}

#[test]
fn setter_waits_for_startup_restore_and_later_pings_preserve_new_setting() -> anyhow::Result<()> {
    let config = recovery_config()?;
    let path = config.expert_settings.clone();
    let backend = PiBackend::new(config)?;
    assert!(!backend.experts_enabled(Some(false))?);
    // Cross several periodic health pings; stale restore work must not overwrite the setter.
    wait_pings(&backend, 4)?;
    assert!(!backend.actual_experts()?.enabled);
    assert!(load(&path)?.is_some_and(|saved| !saved.enabled && saved.prompt == "saved prompt"));
    assert_eq!(backend.status().experts_synced, Some(true));
    backend.shutdown();
    fs::remove_dir_all(path.parent().ok_or_else(|| anyhow::anyhow!("missing parent"))?)?;
    Ok(())
}

//! Host settings: POLARIS_AGENT_CONFIG JSON, with explicit environment overrides.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use super::BackendError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind { Pi, ClaudeCli }
impl BackendKind {
    pub fn parse(value: &str) -> Result<Self, BackendError> {
        match value {
            "pi" => Ok(Self::Pi),
            "claude-cli" => Ok(Self::ClaudeCli),
            other => Err(BackendError::InvalidBackend(other.into())),
        }
    }
}
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub agent: Agent,
    pub agentd: Agentd,
}
impl Default for Config {
    fn default() -> Self { Self { agent: Agent::default(), agentd: Agentd::default() } }
}
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Agent { pub backend: String }
impl Default for Agent { fn default() -> Self { Self { backend: "pi".into() } } }
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Agentd {
    pub node: PathBuf,
    pub entry: PathBuf,
    pub node_args: Vec<String>,
    pub args: Vec<String>,
    pub isolation: String,
    pub max_processes: usize,
    pub idle_ms: u64,
    pub request_timeout_ms: u64,
    /// Lost-daemon fallback: 240000 > agentd's 180000 so agentd can finish first.
    pub turn_idle_timeout_ms: u64,
    /// Hard turn limit in milliseconds; zero disables it.
    pub turn_max_ms: u64,
    #[serde(rename = "turnTimeoutMs", deserialize_with = "reject_turn_timeout", skip_serializing)]
    _legacy_turn_timeout: (),
    pub abort_timeout_ms: u64,
    pub ping_interval_ms: u64,
    pub restart_base_ms: u64,
    pub restart_max_ms: u64,
    pub restart_jitter_ms: u64,
    pub failure_threshold: u32,
    pub stable_reset_ms: u64,
    pub channel_capacity: usize,
    pub max_line_bytes: usize,
    pub delta_batch_ms: u64,
    pub write_tools: Vec<String>,
    pub readonly_tools: Vec<String>,
    pub supervisor_tick_ms: u64,
    pub shutdown_timeout_ms: u64,
    pub control_poll_ms: u64,
    pub session_map: PathBuf,
}
impl Default for Agentd {
    fn default() -> Self {
        Self {
            node: "node".into(), entry: PathBuf::new(), node_args: vec![], args: vec![],
            isolation: if cfg!(feature = "desktop") { "shared" } else { "per-account" }.into(),
            max_processes: 8, idle_ms: 600_000, request_timeout_ms: 15_000,
            turn_idle_timeout_ms: 240_000, turn_max_ms: 0, _legacy_turn_timeout: (),
            abort_timeout_ms: 5_000, ping_interval_ms: 30_000,
            restart_base_ms: 200, restart_max_ms: 30_000, restart_jitter_ms: 100,
            failure_threshold: 5, stable_reset_ms: 60_000, channel_capacity: 64,
            max_line_bytes: 1_048_576, delta_batch_ms: 30,
            write_tools: vec!["write".into(), "edit".into()],
            readonly_tools: vec!["read".into(), "grep".into(), "find".into(), "ls".into()],
            supervisor_tick_ms: 10, shutdown_timeout_ms: 1000, control_poll_ms: 1,
            session_map: crate::runtime::paths::data_dir().join("agent-session-map"),
        }
    }
}
fn reject_turn_timeout<'de, D: serde::Deserializer<'de>>(_: D) -> Result<(), D::Error> {
    Err(serde::de::Error::custom(
        "InvalidAgentConfig: agentd.turnTimeoutMs was removed; use turnIdleTimeoutMs and turnMaxMs",
    ))
}
impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let mut config: Self = match std::env::var_os("POLARIS_AGENT_CONFIG") {
            Some(path) => serde_json::from_slice(&std::fs::read(path)?)?,
            None => Self::default(),
        };
        if let Ok(v) = std::env::var("POLARIS_AGENT_BACKEND") { config.agent.backend = v; }
        if let Some(v) = std::env::var_os("POLARIS_AGENTD_NODE") { config.agentd.node = v.into(); }
        if let Some(v) = std::env::var_os("POLARIS_AGENTD_ENTRY") { config.agentd.entry = v.into(); }
        if let Ok(v) = std::env::var("POLARIS_AGENTD_ARGS") { config.agentd.args = serde_json::from_str(&v)?; }
        if let Ok(v) = std::env::var("POLARIS_AGENTD_NODE_ARGS") { config.agentd.node_args = serde_json::from_str(&v)?; }
        if let Ok(v) = std::env::var("POLARIS_AGENTD_ISOLATION") { config.agentd.isolation = v; }
        config.validate()?;
        Ok(config)
    }
    pub fn validate(&self) -> anyhow::Result<()> {
        BackendKind::parse(&self.agent.backend)?;
        let c = &self.agentd;
        anyhow::ensure!(matches!(c.isolation.as_str(), "shared" | "per-account"), "InvalidIsolation: {}", c.isolation);
        anyhow::ensure!(c.channel_capacity > 0 && c.max_line_bytes > 0 && c.max_processes > 0,
            "InvalidAgentConfig: capacities must be positive");
        anyhow::ensure!(c.turn_idle_timeout_ms > 0, "InvalidAgentConfig: agentd.turnIdleTimeoutMs must be positive");
        anyhow::ensure!(c.request_timeout_ms > 0 && c.abort_timeout_ms > 0
            && c.ping_interval_ms > 0 && c.idle_ms > 0 && c.delta_batch_ms > 0
            && c.supervisor_tick_ms > 0 && c.control_poll_ms > 0 && c.shutdown_timeout_ms > 0
            && c.stable_reset_ms > 0 && c.failure_threshold > 0 && c.restart_base_ms > 0 && c.restart_max_ms >= c.restart_base_ms,
            "InvalidAgentConfig: timeouts and restart bounds must be positive");
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn backend_selection_is_explicit() {
        assert_eq!(BackendKind::parse(&Config::default().agent.backend).unwrap(), BackendKind::Pi);
        assert_eq!(BackendKind::parse("claude-cli").unwrap(), BackendKind::ClaudeCli);
        assert!(BackendKind::parse("typo").unwrap_err().to_string().contains("InvalidBackend"));
    }
}

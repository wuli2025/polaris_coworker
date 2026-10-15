//! Persisted host override, delivered using config.apply and verified using ping.
use super::{PiBackend, Command, Empty};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertConfig { pub enabled: bool, pub prompt: String }

const DEFAULT_PROMPT_MAX_BYTES: usize = 32768;
const PROMPT_LIMIT_ENV: &str = "POLARIS_EXPERT_PROMPT_MAX_BYTES";

fn parse_prompt_limit(value: Option<&str>) -> anyhow::Result<usize> {
    match value {
        None => Ok(DEFAULT_PROMPT_MAX_BYTES),
        Some(value) => value.parse::<usize>().ok().filter(|limit| *limit > 0)
            .ok_or_else(|| anyhow::anyhow!("ExpertPromptLimitInvalid: {PROMPT_LIMIT_ENV} must be a positive byte count")),
    }
}

pub(super) fn prompt_limit() -> anyhow::Result<usize> {
    match std::env::var(PROMPT_LIMIT_ENV) {
        Ok(value) => parse_prompt_limit(Some(&value)),
        Err(std::env::VarError::NotPresent) => parse_prompt_limit(None),
        Err(_) => anyhow::bail!("ExpertPromptLimitInvalid: {PROMPT_LIMIT_ENV} must be UTF-8"),
    }
}

impl ExpertConfig {
    pub(super) fn normalized(mut self, limit: usize) -> anyhow::Result<Self> {
        anyhow::ensure!(self.prompt.len() <= limit,
            "ExpertPromptTooLong: {} bytes exceeds limit {limit}", self.prompt.len());
        self.prompt.truncate(self.prompt.trim_end().len());
        Ok(self)
    }
}

pub fn load(path: &Path) -> anyhow::Result<Option<ExpertConfig>> {
    load_with_limit(path, prompt_limit()?)
}

fn load_with_limit(path: &Path, limit: usize) -> anyhow::Result<Option<ExpertConfig>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(serde_json::from_slice::<ExpertConfig>(&bytes)
            .map_err(|e| anyhow::anyhow!("ExpertConfigInvalid: {e}"))?.normalized(limit)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(anyhow::anyhow!("ExpertConfigReadFailed: {e}")),
    }
}

fn persist(path: &Path, config: &ExpertConfig) -> anyhow::Result<()> {
    let config = config.clone().normalized(prompt_limit()?)?;
    if let Some(parent) = path.parent() { fs::create_dir_all(parent)?; }
    crate::provider::atomic_write(path, &serde_json::to_string_pretty(&config)?)
        .map_err(|e| anyhow::anyhow!("ExpertConfigWriteFailed: {e}"))
}

impl PiBackend {
    fn actual_experts(&self) -> anyhow::Result<ExpertConfig> {
        let ping = self.client()?.call(Command::Ping(Empty {}))?;
        super::protocol::check_ping(ping)?.experts
            .ok_or_else(|| anyhow::anyhow!("ExpertConfigReadUnsupported"))?
            .normalized(prompt_limit()?)
    }

    fn apply_experts(&self, config: &ExpertConfig) -> anyhow::Result<()> {
        let config = config.clone().normalized(prompt_limit()?)?;
        self.apply_config(Some(serde_json::json!({ "experts": config })), None)?;
        anyhow::ensure!(self.actual_experts()? == config, "ExpertConfigApplyMismatch");
        Ok(())
    }

    pub(super) fn expert_state(&self, enabled: Option<bool>) -> anyhow::Result<bool> {
        let _guard = self.expert_lock.lock();
        // Supervisor queues this ping behind any in-flight restore transaction.
        let before = self.actual_experts()?;
        let status = self.client()?.status.lock().clone();
        anyhow::ensure!(status.experts_synced == Some(true), "ExpertConfigUnsynced: {}",
            status.expert_sync_error.as_deref().map_or("awaiting daemon ping", |error| error));
        let Some(enabled) = enabled else { return Ok(before.enabled); };
        let config = ExpertConfig { enabled, prompt: before.prompt.clone() };
        self.apply_experts(&config)?;
        if let Err(error) = persist(&self.config.expert_settings, &config) {
            if let Err(rollback) = self.apply_experts(&before) {
                let client = self.client()?;
                let mut status = client.status.lock();
                status.experts_synced = Some(false);
                status.expert_sync_error = Some(format!("ExpertConfigRollbackFailed: {rollback}"));
                anyhow::bail!("{error}; ExpertConfigRollbackFailed: {rollback}");
            }
            return Err(error);
        }
        Ok(enabled)
    }
}

#[cfg(test)]
#[path = "experts_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "experts_recovery_tests.rs"]
mod recovery_tests;

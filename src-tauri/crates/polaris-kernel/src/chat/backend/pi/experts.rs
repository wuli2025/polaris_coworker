//! Persisted host override, delivered using config.apply and verified using ping.
use super::{PiBackend, Command, Empty};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertConfig { pub enabled: bool, pub prompt: String }

pub fn load(path: &Path) -> anyhow::Result<Option<ExpertConfig>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)
            .map_err(|e| anyhow::anyhow!("ExpertConfigInvalid: {e}"))?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(anyhow::anyhow!("ExpertConfigReadFailed: {e}")),
    }
}

fn persist(path: &Path, config: &ExpertConfig) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() { fs::create_dir_all(parent)?; }
    crate::provider::atomic_write(path, &serde_json::to_string_pretty(config)?)
        .map_err(|e| anyhow::anyhow!("ExpertConfigWriteFailed: {e}"))
}

impl PiBackend {
    fn actual_experts(&self) -> anyhow::Result<ExpertConfig> {
        let ping = self.client()?.call(Command::Ping(Empty {}))?;
        super::protocol::check_ping(ping)?.experts
            .ok_or_else(|| anyhow::anyhow!("ExpertConfigReadUnsupported"))
    }

    fn apply_experts(&self, config: &ExpertConfig) -> anyhow::Result<()> {
        self.apply_config(Some(serde_json::json!({ "experts": config })), None)?;
        anyhow::ensure!(self.actual_experts()? == *config, "ExpertConfigApplyMismatch");
        Ok(())
    }

    fn restore_experts(&self) -> anyhow::Result<()> {
        if let Some(saved) = load(&self.config.expert_settings)? {
            if self.actual_experts()? != saved { self.apply_experts(&saved)?; }
        }
        Ok(())
    }

    pub(super) fn sync_experts(&self) -> anyhow::Result<()> {
        let _guard = self.expert_lock.lock();
        self.restore_experts()
    }

    pub(super) fn expert_state(&self, enabled: Option<bool>) -> anyhow::Result<bool> {
        let _guard = self.expert_lock.lock();
        self.restore_experts()?;
        let before = self.actual_experts()?;
        let Some(enabled) = enabled else { return Ok(before.enabled); };
        let config = ExpertConfig { enabled, prompt: before.prompt.clone() };
        self.apply_experts(&config)?;
        if let Err(error) = persist(&self.config.expert_settings, &config) {
            if let Err(rollback) = self.apply_experts(&before) {
                anyhow::bail!("{error}; ExpertConfigRollbackFailed: {rollback}");
            }
            return Err(error);
        }
        Ok(enabled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::backend::AgentBackend;

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
}

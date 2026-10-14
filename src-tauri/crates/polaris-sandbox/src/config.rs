//! Docker policy. Configuration may add protected paths, never remove the baseline.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct Config {
    pub mounts: Vec<Mount>,
    pub protected_paths: Vec<PathBuf>,
    pub memory_bytes: u64,
    pub cpus: f64,
    pub cap_add: Vec<String>,
    pub workdir_change: WorkdirChange,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Mount {
    /// ${project}, ${kb}, or an absolute path matching the requested root.
    pub source: String,
    pub destination: String,
    pub read_only: bool,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum WorkdirChange { Reject, Rebuild }
impl Default for Config {
    fn default() -> Self {
        Self {
            mounts: vec![
                Mount { source: "${project}".into(), destination: "/workspace".into(), read_only: false },
                Mount { source: "${kb}".into(), destination: "/kb".into(), read_only: true },
            ],
            protected_paths: Vec::new(), memory_bytes: 4 * 1024 * 1024 * 1024,
            cpus: 2.0, cap_add: Vec::new(), workdir_change: WorkdirChange::Reject,
        }
    }
}
impl Config {
    pub fn load() -> Result<Self> {
        let config = match std::env::var_os("POLARIS_SANDBOX_CONFIG") {
            Some(path) => serde_json::from_slice(&std::fs::read(path)?)?,
            None => Self::default(),
        };
        config.validate()?;
        Ok(config)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(self.memory_bytes >= 6 * 1024 * 1024 && self.memory_bytes <= i64::MAX as u64,
            "SANDBOX_INVALID_CONFIG: memoryBytes must be 6 MiB..i64::MAX");
        ensure!(self.cpus.is_finite() && self.cpus >= 0.01 && self.cpus <= 1024.0,
            "SANDBOX_INVALID_CONFIG: cpus must be 0.01..1024");
        // Only narrowly scoped additions; administrative capabilities are never configurable.
        ensure!(self.cap_add.iter().all(|c| ["CHOWN", "FOWNER", "DAC_OVERRIDE", "NET_BIND_SERVICE"].contains(&c.as_str())),
            "SANDBOX_INVALID_CONFIG: unsupported capAdd");
        ensure!(self.mounts.len() == 2, "SANDBOX_INVALID_CONFIG: exactly /workspace and /kb required");
        for (destination, read_only) in [("/workspace", false), ("/kb", true)] {
            ensure!(self.mounts.iter().filter(|m| m.destination == destination && m.read_only == read_only).count() == 1,
                "SANDBOX_INVALID_CONFIG: /workspace must be writable and /kb read-only");
        }
        ensure!(self.protected_paths.iter().all(|p| p.is_absolute()),
            "SANDBOX_INVALID_CONFIG: protectedPaths must be absolute");
        Ok(())
    }
    pub fn protected(&self, home: &Path) -> Vec<PathBuf> {
        let mut paths: Vec<_> = ["Polaris/data", "Polaris/claude-home", "Polaris/sessions", "Polaris/conversations",
            ".polaris", ".pi", ".claude", ".codex", ".ssh", ".aws", ".gnupg", ".kube"]
            .iter().map(|p| home.join(p)).collect();
        paths.extend(self.protected_paths.iter().cloned());
        paths.extend(["/var/run", "/run", "/proc", "/sys", "/dev"].iter().map(PathBuf::from));
        paths
    }
}

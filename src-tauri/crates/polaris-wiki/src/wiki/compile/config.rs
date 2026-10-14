//! Wiki-only choices; no global chat permission or sandbox setting is changed.
use serde::Deserialize;
#[derive(Clone, Debug, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct CompileConfig {
    pub tool_profile: String,
    pub sandbox: bool,
    pub timeout_secs: u64,
    pub abort_grace_ms: u64,
}
impl Default for CompileConfig {
    fn default() -> Self {
        Self { tool_profile: "workspace".into(), sandbox: false, timeout_secs: 3600, abort_grace_ms: 5000 }
    }
}
impl CompileConfig {
    pub fn load() -> anyhow::Result<Self> {
        let mut config: Self = match std::env::var_os("POLARIS_WIKI_CONFIG") {
            Some(path) => serde_json::from_slice(&std::fs::read(path)?)?,
            None => Self::default(),
        };
        if let Ok(value) = std::env::var("POLARIS_KB_COMPILE_CAP_SECS") {
            config.timeout_secs = value.parse()?;
        }
        config.validate()?;
        Ok(config)
    }
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(!self.tool_profile.trim().is_empty(), "InvalidWikiConfig: toolProfile must not be empty");
        anyhow::ensure!(self.timeout_secs > 0 && self.timeout_secs <= 604800,
            "InvalidWikiConfig: timeoutSecs must be 1..=604800");
        anyhow::ensure!((1..=60000).contains(&self.abort_grace_ms),
            "InvalidWikiConfig: abortGraceMs must be 1..=60000");
        Ok(())
    }
}

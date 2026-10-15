//! Host -> child transport settings. Never include proxy credentials in Debug.
use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Network {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idle_timeout_ms: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub probe_timeout_ms: Option<u32>,
}
impl std::fmt::Debug for Network {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Network").field("proxy_configured", &self.proxy.is_some())
            .field("idle_timeout_ms", &self.idle_timeout_ms).field("probe_timeout_ms", &self.probe_timeout_ms).finish()
    }
}
impl Network {
    pub fn validate(&self) -> anyhow::Result<()> {
        if let Some(proxy) = &self.proxy {
            let valid = ureq::get(proxy).request_url().ok().is_some_and(|url| {
                matches!(url.scheme(), "http" | "https") && !url.host().is_empty()
                    && !proxy.contains('?') && !proxy.contains('#') && url.path() == "/"
            });
            anyhow::ensure!(valid, "AgentNetworkConfigInvalid [paths: /agentd/network/proxy]");
        }
        anyhow::ensure!(self.idle_timeout_ms.is_none_or(|n| n <= i32::MAX as u32),
            "AgentNetworkConfigInvalid [paths: /agentd/network/idleTimeoutMs]");
        anyhow::ensure!(self.probe_timeout_ms.is_none_or(|n| n > 0 && n <= i32::MAX as u32),
            "AgentNetworkConfigInvalid [paths: /agentd/network/probeTimeoutMs]");
        Ok(())
    }
    pub fn configure(&self, command: &mut std::process::Command) -> anyhow::Result<()> {
        self.validate()?;
        command.env("POLARIS_AGENTD_NETWORK", serde_json::to_string(self)?);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn forwards_settings_and_redacts_diagnostics() -> anyhow::Result<()> {
        let network: Network = serde_json::from_str(r#"{"proxy":"http://user:secret@localhost:8080","idleTimeoutMs":0}"#)?;
        let mut command = std::process::Command::new("node");
        network.configure(&mut command)?;
        assert!(!format!("{network:?}").contains("secret"));
        let value = command.get_envs().find(|(key, _)| *key == "POLARIS_AGENTD_NETWORK")
            .and_then(|(_, value)| value).ok_or_else(|| anyhow::anyhow!("missing child configuration"))?;
        let decoded: Network = serde_json::from_str(&value.to_string_lossy())?;
        assert_eq!(decoded.proxy, network.proxy);
        assert_eq!(decoded.idle_timeout_ms, Some(0));
        let bad = Network { proxy: Some("socks5://secret@localhost:80".into()), ..Default::default() };
        let error = bad.validate().err().ok_or_else(|| anyhow::anyhow!("invalid proxy accepted"))?;
        assert!(error.to_string().contains("/agentd/network/proxy"));
        assert!(!error.to_string().contains("secret"));
        Ok(())
    }
}

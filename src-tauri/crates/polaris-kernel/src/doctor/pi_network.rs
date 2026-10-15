//! Run a bounded, unauthenticated HEAD through agentd's actual dispatcher.
use super::*;
use serde::Deserialize;

const PROXY_HELP: &str = "支持 HTTP_PROXY/HTTPS_PROXY/NO_PROXY 或 POLARIS_AGENT_CONFIG 的 agentd.network.proxy；Windows 系统代理/PAC 不会自动读取，请配置代理软件的 HTTP 监听地址并重启 Polaris";
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reachability { reachable: bool, proxy: bool }

pub(super) fn check() -> VerifyStep {
    let start = std::time::Instant::now();
    let result = (|| -> Result<Reachability, String> {
        let config = Config::load().map_err(|_| "AgentConfigInvalid")?;
        let (_, selected) = crate::provider::selection::snapshot();
        let views = crate::provider::store::pi_views();
        let view = views.iter().find(|view| view.id == selected).ok_or("ProviderNotFound")?;
        let target = if view.base_url.is_empty() || matches!(view.id.as_str(), "codex" | "claude-official") {
            crate::provider::pi_translate::credential_id(&view.id)
        } else { &view.base_url };
        probe(&config.agentd, target)
    })();
    let (status, detail) = match result {
        Ok(result) => (if result.reachable { "ok" } else { "fail" }, format!("{}；代理={}。{PROXY_HELP}",
            if result.reachable { "可达" } else { "不可达" }, if result.proxy { "是" } else { "否" })),
        Err(_) => ("fail", format!("不可达（探测未完成）；代理=未知。{PROXY_HELP}")),
    };
    VerifyStep { key: "model-network".into(), name: "模型网络连通性".into(), status: status.into(),
        detail, command: None, output: None, ms: start.elapsed().as_millis() as u64 }
}
fn probe(config: &Agentd, target: &str) -> Result<Reachability, String> {
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()
        .map_err(|_| "AgentdProbeRuntimeFailed")?;
    runtime.block_on(async {
        let mut command = tokio::process::Command::new(&config.node);
        command.args(&config.node_args).arg(&config.entry).arg("--probe-network").arg(target)
            .args(&config.args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true);
        config.network.configure(command.as_std_mut()).map_err(|_| "AgentNetworkConfigInvalid")?;
        crate::runtime::procs::no_window(command.as_std_mut());
        let mut child = command.spawn().map_err(|_| "AgentdSpawnFailed")?;
        let outcome = tokio::time::timeout(Duration::from_millis(config.request_timeout_ms), async {
            let output = child.stdout.take().ok_or("AgentdStdoutMissing")?;
            let mut reader = BufReader::new(output.take(config.max_line_bytes as u64 + 1));
            let mut bytes = Vec::new();
            reader.read_until(b'\n', &mut bytes).await.map_err(|_| "AgentdReadFailed")?;
            if bytes.len() > config.max_line_bytes { return Err("AgentdInvalidFrame"); }
            serde_json::from_slice(&bytes).map_err(|_| "AgentdInvalidFrame")
        }).await;
        let _ = child.start_kill();
        let _ = tokio::time::timeout(Duration::from_millis(config.shutdown_timeout_ms), child.wait()).await;
        outcome.map_err(|_| "AgentdProbeTimeout".to_string())?.map_err(str::to_owned)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn probe_passes_host_proxy_and_returns_only_reachability() -> Result<(), Box<dyn std::error::Error>> {
        let file = std::env::temp_dir().join(format!("p7-doctor-network-{}.mjs", std::process::id()));
        std::fs::write(&file, r#"
const config = JSON.parse(process.env.POLARIS_AGENTD_NETWORK);
console.log(JSON.stringify({reachable: process.argv[2] === '--probe-network', proxy: !!config.proxy}));
"#)?;
        let mut config = Agentd::default();
        config.entry = file.clone();
        config.network.proxy = Some("http://user:secret@localhost:80".into());
        let result = probe(&config, "https://example.invalid")?;
        assert!(result.reachable && result.proxy);
        std::fs::remove_file(file)?;
        Ok(())
    }
}

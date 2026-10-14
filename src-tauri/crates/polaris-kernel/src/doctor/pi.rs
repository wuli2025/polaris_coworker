//! Pi diagnostics never install software or disclose credential values.
use super::types::*;
use crate::chat::backend::{config::{Config, Agentd}, pi::protocol::{check_ping, Ping}};
use serde::Serialize;
use serde_json::Value;
use std::{process::Stdio, time::Duration};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AgentRuntimeReport {
    pub backend: String,
    pub node_ready: bool,
    pub entry_exists: bool,
    pub handshake: bool,
    pub protocol_version: Option<u64>,
    pub pi_version: Option<String>,
    pub agentd_version: Option<String>,
    pub credentials_configured: bool,
    pub legacy_actions: bool,
    pub error: Option<String>,
}
impl AgentRuntimeReport {
    pub fn ready(&self) -> bool {
        self.node_ready && self.entry_exists && self.handshake && self.credentials_configured
    }
}
pub fn require_legacy() -> Result<(), String> {
    let config = Config::load().map_err(|_| "AgentConfigInvalid".to_string())?;
    if config.agent.backend != "claude-cli" { return Err("LegacyBackendRequired: agent.backend=claude-cli".into()); }
    Ok(())
}
pub fn legacy_enabled() -> bool { require_legacy().is_ok() }

pub(crate) fn credentials_ready(root: &Value) -> bool {
    root.get("credentials").and_then(Value::as_object).is_some_and(|entries| entries.values().any(|c| {
        let nonempty = |key| c.get(key).and_then(Value::as_str).is_some_and(|s| !s.trim().is_empty());
        match c.get("type").and_then(Value::as_str) {
            Some("api_key") => nonempty("key"),
            Some("oauth") => nonempty("access") && nonempty("refresh"),
            _ => false,
        }
    }))
}
fn store_ready() -> bool {
    crate::provider::credential_file::provider_store_path()
        .and_then(|p| crate::provider::credential_file::read_root(&p).ok())
        .is_some_and(|v| credentials_ready(&v))
}
pub(crate) fn node_supported(version: &str) -> bool {
    let parse = |s: &str| -> Option<Vec<u64>> {
        let v = s.trim().trim_start_matches('v').split('.').map(str::parse).collect::<Result<Vec<u64>, _>>().ok()?;
        (v.len() == 3).then_some(v)
    };
    let manifest: Result<Value, _> = serde_json::from_str(include_str!("../../../../../agentd/package.json"));
    let minimum = manifest.ok().and_then(|v| v["engines"]["node"].as_str().map(str::to_owned));
    match (parse(version), minimum.as_deref().and_then(|s| s.strip_prefix(">=")).and_then(parse)) {
        (Some(actual), Some(minimum)) => actual >= minimum,
        _ => false,
    }
}
/// Bounded handshake on a short-lived daemon: no session, model call, or credentials written.
pub fn ping(config: &Agentd) -> Result<Ping, String> {
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()
        .map_err(|_| "AgentdProbeRuntimeFailed")?;
    runtime.block_on(async {
        let mut cmd = tokio::process::Command::new(&config.node);
        cmd.args(&config.node_args).arg(&config.entry).args(&config.args)
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true);
        #[cfg(windows)]
        super::pi_shell::configure(cmd.as_std_mut(), config)?;
        crate::runtime::procs::no_window(cmd.as_std_mut());
        let mut child = cmd.spawn().map_err(|_| "AgentdSpawnFailed")?;
        let result = tokio::time::timeout(Duration::from_millis(config.request_timeout_ms), async {
            let mut input = child.stdin.take().ok_or("AgentdStdinMissing")?;
            let output = child.stdout.take().ok_or("AgentdStdoutMissing")?;
            input.write_all(b"{\"id\":\"doctor\",\"method\":\"ping\",\"params\":{}}\n")
                .await.map_err(|_| "AgentdWriteFailed")?;
            let mut reader = BufReader::new(output.take(config.max_line_bytes as u64 + 1));
            let mut line = Vec::new();
            reader.read_until(b'\n', &mut line).await.map_err(|_| "AgentdReadFailed")?;
            if line.len() > config.max_line_bytes || line.last() != Some(&b'\n') { return Err("AgentdInvalidFrame"); }
            let frame: Value = serde_json::from_slice(&line).map_err(|_| "AgentdInvalidFrame")?;
            if frame["type"] != "response" || frame["id"] != "doctor" || frame["ok"] != true { return Err("AgentdPingFailed"); }
            check_ping(frame["result"].clone()).map_err(|_| "AgentdProtocolOrVersionMismatch")
        }).await;
        let _ = child.start_kill();
        let _ = tokio::time::timeout(Duration::from_millis(config.shutdown_timeout_ms), child.wait()).await;
        match result { Ok(r) => r.map_err(str::to_owned), Err(_) => Err("AgentdPingTimeout".into()) }
    })
}
fn absent(key: &str) -> ToolStatus {
    ToolStatus { key: key.into(), name: key.into(), found: false, version: None, path: None,
        on_path: false, required: false, hint: "非当前后端必需组件".into(), bundled: false }
}
pub(crate) fn inspect(config: &Agentd, credentials: bool) -> (ToolStatus, AgentRuntimeReport) {
    let mut runtime = AgentRuntimeReport { backend: "pi".into(), ..Default::default() };
    let mut node = absent("node");
    node.required = true;
    let version = super::probe::probe_version_at(&config.node, &["--version"]);
    runtime.node_ready = version.as_deref().is_some_and(node_supported);
    node.found = runtime.node_ready;
    node.version = version;
    node.path = Some(config.node.to_string_lossy().into());
    node.bundled = super::bundled::is_bundled(&config.node);
    node.hint = if node.found { "Node 已就绪" } else { "Node 缺失、不可执行或版本过低" }.into();
    runtime.entry_exists = config.entry.is_file();
    runtime.credentials_configured = credentials;
    if runtime.node_ready && runtime.entry_exists {
        match ping(config) {
            Ok(ping) => {
                let manifest: Option<Value> = serde_json::from_str(include_str!("../../../../../agentd/package.json")).ok();
                runtime.handshake = manifest.as_ref().and_then(|v| v["dependencies"]["@earendil-works/pi-coding-agent"].as_str())
                    == Some(ping.pi_version.as_str());
                if !runtime.handshake { runtime.error = Some("AgentdPiVersionMismatch".into()); }
                runtime.protocol_version = Some(ping.protocol_version);
                runtime.pi_version = Some(ping.pi_version);
                runtime.agentd_version = Some(ping.version);
            }
            Err(error) => runtime.error = Some(error),
        }
    } else { runtime.error = Some("AgentRuntimeMissing".into()); }
    (node, runtime)
}
pub fn report() -> EnvReport {
    let mut runtime = AgentRuntimeReport { backend: "pi".into(), ..Default::default() };
    let mut node = absent("node");
    node.required = true;
    let mut shell = None;
    match Config::load() {
        Err(_) => runtime.error = Some("AgentConfigInvalid".into()),
        Ok(config) => {
            (node, runtime) = inspect(&config.agentd, store_ready());
            match super::pi_shell::resolve(&config.agentd) {
                Ok(found) => shell = found,
                Err(error) => runtime.error = Some(error),
            }
        }
    }
    let shell_ready = shell.is_some();
    let shell_bundled = shell.as_deref().is_some_and(super::bundled::is_bundled);
    if !shell_ready && runtime.error.is_none() { runtime.error = Some("AgentShellMissing".into()); }
    let ready = runtime.ready() && shell_ready;
    let uv = super::probe::detect("uv", "uv", "uv", &["--version"], &super::probe::uv_candidates(), false, "可选组件未配置");
    let python = super::probe::detect("python", "Python", "python", &["--version"], &super::probe::python_candidates(), false, "可选组件未配置");
    EnvReport { os: std::env::consts::OS.into(), claude: absent("claude"), pwsh: absent("pwsh"), node,
        npm: absent("npm"), uv, python, claude_dir: None,
        claude_dir_on_user_path: true, shell_ready, shell_bundled, ready,
        image_managed: !cfg!(feature = "desktop"), agent_runtime: runtime }
}
pub fn verify() -> VerifyReport {
    let report = report();
    let r = &report.agent_runtime;
    let steps = [("node", r.node_ready), ("agentd-entry", r.entry_exists), ("ping", r.handshake),
        ("credentials", r.credentials_configured), ("shell", report.shell_ready)].into_iter().map(|(key, ok)| VerifyStep {
            key: key.into(), name: key.into(), status: if ok { "ok" } else { "fail" }.into(),
            detail: if key == "shell" {
                if !ok { "未找到 shell；请配置 shellPath、安装 Git Bash 或将 bash 加入 PATH" }
                else if report.shell_bundled { "使用随包 MinGit Bash" }
                else { "使用显式配置或系统 shell（Windows：Program Files Git Bash → PATH）" }
            } else if ok { "已配置" } else { "未配置" }.into(), command: None, output: None, ms: 0,
        }).collect();
    VerifyReport { os: report.os, ok: report.ready, app_runnable: r.handshake, terminal_runnable: r.node_ready,
        deep: false, steps, conflicts: vec![], summary: if report.ready { "Agent 运行时已就绪" } else { "Agent 运行时未就绪" }.into() }
}
#[cfg(test)]
#[path = "pi_tests.rs"]
mod tests;

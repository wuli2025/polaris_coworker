//! Agent boundary. New daemon I/O uses a dedicated Tokio actor: cancellable
//! reads/writes and bounded queues keep UI threads independent of pipe progress.
//! The legacy adapter retains its existing std::thread implementation verbatim.
pub mod config;
pub mod images;
pub mod chat_input;
pub mod claude_cli;
pub mod pi;
pub mod stream;
mod bridge;
pub mod host_tools;
pub mod expert_selection;
pub mod docker_tools;
mod sandbox_process;
use std::{path::PathBuf, sync::Arc, time::Duration};
use serde::Serialize;
use crate::chat::{ChatSendArgs, PermissionMode};
#[cfg(feature = "desktop")]
use tauri::AppHandle;
#[cfg(not(feature = "desktop"))]
use crate::host::AppHandle;
pub use stream::EventStream;
pub(crate) use bridge::{run_chat, emit as bridge_emit};

#[derive(Debug)]
pub enum BackendError {
    InvalidBackend(String),
    AccountContextRequired,
    UnsupportedIsolation,
    CircuitOpen,
    UnsupportedInput(&'static str),
}
impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidBackend(v) => write!(f, "InvalidBackend: agent.backend={v:?}"),
            Self::AccountContextRequired => write!(f, "AccountContextRequired: per-account needs a trusted login identity"),
            Self::UnsupportedIsolation => write!(f, "UnsupportedIsolation: an OS account sandbox is required"),
            Self::CircuitOpen => write!(f, "AgentdCircuitOpen: consecutive failures exceeded configured threshold"),
            Self::UnsupportedInput(v) => write!(f, "AgentdUnsupportedInput: {v}"),
        }
    }
}
impl std::error::Error for BackendError {}

#[derive(Default)]
pub struct ToolOptions {
    pub sandbox: bool,
    /// Session profile selected by the caller; validated by agentd.
    pub tool_profile: Option<String>,
    /// Preserve tool arguments/completions for non-UI consumers.
    pub detailed_events: bool,
    pub expert_prompt: Option<String>,
}
pub struct TurnRequest {
    pub req_id: String,
    /// Polaris session key; the Pi adapter maps it to the daemon's UUID.
    pub session_id: String,
    pub conversation_id: Option<String>,
    pub prompt: String,
    pub permission: PermissionMode,
    pub model: Option<serde_json::Value>,
    pub cwd: PathBuf,
    /// Validated, encoded image content; adapters never reopen IPC paths.
    pub attachments: Vec<pi::protocol::Attachment>,
    pub thinking_level: Option<pi::protocol::ThinkingLevel>,
    pub account_id: Option<String>,
    pub timeout: Option<Duration>,
    pub(crate) legacy: Option<(AppHandle, ChatSendArgs)>,
}
impl TurnRequest {
    /// Independent non-UI session; callers explicitly choose permissions and tools.
    pub fn headless(req_id: String, cwd: PathBuf, prompt: String, permission: PermissionMode) -> Self {
        Self { session_id: req_id.clone(), req_id, cwd, prompt, permission,
            conversation_id: None, model: None, attachments: vec![], thinking_level: None,
            account_id: None, timeout: None, legacy: None }
    }
}
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(Default))]
#[serde(rename_all = "camelCase")]
pub struct BackendStatus {
    pub backend: String,
    pub agentd_version: Option<String>,
    pub pid: Option<u32>,
    pub restarts: u32,
    pub usage_persist_failures: u64,
    /// None for backends without daemon expert configuration.
    pub experts_synced: Option<bool>,
    pub expert_sync_error: Option<String>,
    pub circuit_open: bool,
    pub healthy: bool,
    pub last_error: Option<String>,
}
impl BackendStatus {
    pub(crate) fn new(name: &str) -> Self {
        Self { backend: name.into(), agentd_version: None, pid: None, restarts: 0,
            usage_persist_failures: 0, experts_synced: (name == "pi").then_some(false),
            expert_sync_error: None, circuit_open: false, healthy: false, last_error: None }
    }
}
#[cfg(test)]
mod status_tests {
    use super::BackendStatus;

    #[test]
    fn explicit_backend_status_reports_expert_support_before_sync() -> serde_json::Result<()> {
        for (backend, synced) in [("pi", Some(false)), ("claude-cli", None)] {
            let status = BackendStatus::new(backend);
            assert_eq!(status.backend, backend);
            assert_eq!(status.experts_synced, synced);
            assert!(!status.healthy);
            assert!(status.expert_sync_error.is_none());
            let value = serde_json::to_value(status)?;
            assert_eq!(value["expertsSynced"], serde_json::to_value(synced)?);
        }
        Ok(())
    }
}
pub trait AgentBackend: Send + Sync {
    fn experts_enabled(&self, _enabled: Option<bool>) -> anyhow::Result<bool> {
        anyhow::bail!("ExpertConfigUnsupported")
    }
    fn provider_apply(&self, _id: &str) -> anyhow::Result<serde_json::Value> {
        anyhow::bail!("ProviderApplyUnsupported")
    }
    fn auth(&self, _operation: &str, _provider: &str, _input: Option<String>) -> anyhow::Result<serde_json::Value> {
        anyhow::bail!("ProviderAuthUnsupported")
    }
    fn permission_respond(&self, _response: pi::protocol::PermissionResponse) -> anyhow::Result<()> {
        anyhow::bail!("PermissionResponseUnsupported")
    }
    fn sandbox_respond(&self, _response: pi::protocol::SandboxResponse) -> anyhow::Result<()> {
        anyhow::bail!("SandboxResponseUnsupported")
    }
    fn start(&self, input: TurnRequest) -> anyhow::Result<EventStream>;
    fn start_with_tools(&self, input: TurnRequest, options: ToolOptions) -> anyhow::Result<EventStream> {
        anyhow::ensure!(!options.sandbox && options.expert_prompt.is_none() && options.tool_profile.is_none() && !options.detailed_events, "BackendToolOptionsUnsupported");
        self.start(input)
    }
    fn live_input(&self, _req: &str, _text: &str, _follow_up: bool) -> anyhow::Result<serde_json::Value> {
        anyhow::bail!("ChatLiveInputUnsupported")
    }
    fn cancel(&self, req_id: &str) -> anyhow::Result<()>;
    fn close(&self, session_id: &str) -> anyhow::Result<()>;
    fn models(&self) -> anyhow::Result<serde_json::Value>;
    fn status(&self) -> BackendStatus;
    fn is_running(&self, req_id: &str) -> bool;
    fn shutdown(&self);
}
static SELECTED: once_cell::sync::OnceCell<Arc<dyn AgentBackend>> = once_cell::sync::OnceCell::new();
pub fn selected() -> anyhow::Result<Arc<dyn AgentBackend>> {
    SELECTED.get_or_try_init(|| {
        let config = config::Config::load()?;
        let backend: Arc<dyn AgentBackend> = match config::BackendKind::parse(&config.agent.backend)? {
            config::BackendKind::Pi => Arc::new(pi::PiBackend::new(config.agentd)?),
            config::BackendKind::ClaudeCli => Arc::new(claude_cli::ClaudeCliBackend),
        };
        Ok(backend)
    }).cloned()
}
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn agent_backend_status() -> Result<BackendStatus, String> {
    selected().map(|b| b.status()).map_err(|e| e.to_string())
}
pub fn close_session(id: &str) {
    if let Some(b) = SELECTED.get() { let _ = b.close(id); }
    else { claude_cli::pool::kill_conv(id); }
}
pub fn shutdown() {
    if let Some(b) = SELECTED.get() { b.shutdown(); }
    claude_cli::pool::kill_all();
}
pub fn collect<F: FnMut(&str, &str)>(
    backend: &dyn AgentBackend, input: TurnRequest, mut on_event: F,
) -> anyhow::Result<String> {
    let req = input.req_id.clone();
    let polling = Duration::from_millis(config::Config::load()?.agentd.request_timeout_ms);
    let deadline = input.timeout.map(|timeout| std::time::Instant::now() + timeout);
    let stream = backend.start(input)?;
    let mut result = String::new();
    let mut error = None;
    loop {
        let event = match stream.recv_timeout(deadline.map(|d| d.saturating_duration_since(std::time::Instant::now()).min(polling)).unwrap_or(polling)) {
            Ok(e) => e,
            Err(e) if matches!(e.downcast_ref(), Some(stream::StreamError::Timeout))
                && deadline.is_none_or(|d| std::time::Instant::now() < d) => continue,
            Err(e) => { let _ = backend.cancel(&req); return Err(e); }
        };
        if host_tools::handle_sandbox(backend, &event)? { continue; }
        match event.kind.as_str() {
            "delta" => { let text = event.text.as_deref().unwrap_or_default(); result.push_str(text); on_event("delta", text); }
            "tool" => on_event("tool", event.tool.as_deref().unwrap_or_default()),
            "error" => error = event.text,
            "done" => return match error { Some(e) => Err(anyhow::anyhow!(e)), None => Ok(result) },
            _ => {}
        }
    }
}
pub(crate) fn event(req: &str, conv: Option<&str>, kind: &str, text: Option<String>) -> crate::chat::ChatStreamEvent {
    crate::chat::ChatStreamEvent { call_id: None, parent_call_id: None, req_id: req.into(), conversation_id: conv.map(str::to_owned), kind: kind.into(), text, tool: None }
}

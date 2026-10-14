//! Host ports: the kernel cannot depend on the sandbox or expert crates.
use super::{AgentBackend, pi::protocol::{PermissionResponse, SandboxResponse}};
use crate::chat::{ChatSendArgs, ChatStreamEvent};
use serde::Deserialize;
use serde_json::Value;
use std::sync::{Arc, OnceLock};

#[derive(Debug, Default)]
pub struct SandboxOutput {
    pub output: String, pub exit_code: Option<i32>, pub signal: Option<i32>, pub timed_out: bool,
}
pub trait SandboxExecutor: Send + Sync {
    fn execute(&self, operation: &str, args: &Value) -> anyhow::Result<SandboxOutput>;
}
static SANDBOX: OnceLock<Arc<dyn SandboxExecutor>> = OnceLock::new();
pub fn set_sandbox_executor(executor: Arc<dyn SandboxExecutor>) {
    if SANDBOX.set(executor).is_err() {
        eprintln!("{}", serde_json::json!({"level":"warn", "component":"sandbox",
            "message":"set_sandbox_executor failed: executor is already installed"}));
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Request { session_id: String, call_id: String, operation: String, args: Value }

pub fn sandbox_response(executor: Option<&dyn SandboxExecutor>, payload: &str) -> anyhow::Result<SandboxResponse> {
    let request: Request = serde_json::from_str(payload)?;
    let result = executor.ok_or_else(|| anyhow::anyhow!("SANDBOX_UNAVAILABLE: host executor is not installed"))
        .and_then(|executor| executor.execute(&request.operation, &request.args));
    let (output, error) = match result { Ok(output) => (output, None), Err(error) => (SandboxOutput::default(), Some(error.to_string())) };
    Ok(SandboxResponse { session_id: request.session_id, call_id: request.call_id, output: output.output, error,
        exit_code: output.exit_code, signal: output.signal, timed_out: output.timed_out })
}
pub fn handle_sandbox(backend: &dyn AgentBackend, event: &ChatStreamEvent) -> anyhow::Result<bool> {
    if event.kind != "sandbox.request" { return Ok(false); }
    let payload = event.text.as_deref().ok_or_else(|| anyhow::anyhow!("SandboxMissingRequest"))?;
    if let Err(error) = backend.sandbox_respond(sandbox_response(SANDBOX.get().map(|e| e.as_ref()), payload)?) {
        // Cancellation can finish the daemon request while Docker is winding down.
        // Preserve the queued terminal event instead of losing it on a stale reply.
        if !error.to_string().starts_with("SANDBOX_REQUEST_NOT_FOUND:") { return Err(error); }
    }
    Ok(true)
}
/// Minimal host confirmation API, bound to authenticated shell IPC.
/// Without a response, agentd denies on timeout; headless callers never auto-approve.
#[cfg_attr(feature = "desktop", tauri::command(async))]
pub fn chat_permission_respond(response: PermissionResponse) -> Result<(), String> {
    super::selected().and_then(|backend| backend.permission_respond(response)).map_err(|error| error.to_string())
}
pub fn expert_prompt(args: &ChatSendArgs) -> Option<String> {
    let bridge = crate::chat::bridges::expert_bridge()?;
    match args.agent_mode.as_deref() {
        Some("single-agent") | Some("single-expert") => None,
        Some("expert-team") if bridge.detect_multi_expert_task(&args.prompt) => {
            let project = args.conversation_id.as_deref().and_then(crate::conv::project_id_of_conversation)?;
            bridge.team_block_spawn(project, args.prompt.clone())
        }
        _ => bridge.route_block(&args.prompt),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fake(std::sync::atomic::AtomicUsize);
    impl SandboxExecutor for Fake {
        fn execute(&self, operation: &str, args: &Value) -> anyhow::Result<SandboxOutput> {
            assert_eq!(operation, "bash");
            assert_eq!(args["command"], "touch /host-marker");
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(SandboxOutput { output: "container-only".into(), exit_code: Some(7), signal: None, timed_out: false })
        }
    }
    #[test]
    fn sandbox_routes_only_through_injected_executor_and_fails_closed() -> anyhow::Result<()> {
        let payload = serde_json::json!({"sessionId":"s","callId":"c", "operation":"bash",
            "args":{"command":"touch /host-marker"}}).to_string();
        let fake = Fake(std::sync::atomic::AtomicUsize::new(0));
        let response = sandbox_response(Some(&fake), &payload)?;
        assert_eq!(response.output, "container-only");
        assert_eq!(response.exit_code, Some(7));
        assert!(response.error.is_none());
        assert_eq!(fake.0.load(std::sync::atomic::Ordering::SeqCst), 1);
        let unavailable = sandbox_response(None, &payload)?;
        assert!(unavailable.error.is_some_and(|e| e.contains("SANDBOX_UNAVAILABLE")));
        assert!(unavailable.output.is_empty());
        Ok(())
    }
}

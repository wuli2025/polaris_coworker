use super::protocol::{Event, TurnStatus};
use crate::chat::{backend::event, artifacts::{is_displayable_artifact, packaged_project_root}, ChatStreamEvent};
use std::{path::PathBuf, collections::HashMap, time::{Duration, Instant}};
use super::super::config::Agentd;

pub struct Translator {
    req: String, conv: Option<String>, cwd: PathBuf,
    tools: HashMap<String, String>, write_tools: Vec<String>,
    delta: String, last_flush: Instant, window: Duration, max_bytes: usize,
    error_seen: bool, done: bool, detailed: bool,
    provider: Option<String>, failure_code: Option<String>,
}
impl Translator {
    pub fn new(req: String, conv: Option<String>, cwd: PathBuf, config: &Agentd) -> Self {
        Self { req, conv, cwd, tools: HashMap::new(), write_tools: config.write_tools.clone(),
            delta: String::new(), last_flush: Instant::now(), window: Duration::from_millis(config.delta_batch_ms),
            max_bytes: config.max_line_bytes, error_seen: false, done: false, detailed: false, provider: None, failure_code: None }
    }
    pub fn with_details(mut self, detailed: bool) -> Self { self.detailed = detailed; self }
    pub fn with_provider(mut self, id: &str) -> Self { self.provider = Some(id.into()); self }
    pub fn make(&self, kind: &str, text: Option<String>) -> ChatStreamEvent { event(&self.req, self.conv.as_deref(), kind, text) }
    pub fn flush(&mut self) -> Vec<ChatStreamEvent> {
        self.last_flush = Instant::now();
        if self.delta.is_empty() { vec![] } else {
            let text = std::mem::take(&mut self.delta);
            vec![self.make("delta", Some(text))]
        }
    }
    pub fn tick(&mut self) -> Vec<ChatStreamEvent> {
        if self.last_flush.elapsed() >= self.window { self.flush() } else { vec![] }
    }
    pub fn accept(&mut self, input: Event) -> Vec<ChatStreamEvent> {
        if self.done { return vec![]; }
        if let Event::Text { text } = input {
            let mut out = if self.delta.len() + text.len() > self.max_bytes { self.flush() } else { vec![] };
            self.delta.push_str(&text);
            out.extend(self.tick());
            return out;
        }
        let mut out = self.flush();
        match input {
            Event::PermissionRequest { target_session_id, call_id, tool, summary, expires_at, parent_call_id } => {
                out.push(self.make("permission.request", Some(serde_json::json!({
                    "sessionId": target_session_id, "callId": call_id, "tool": tool,
                    "summary": summary, "expiresAt": expires_at, "parentCallId": parent_call_id,
                }).to_string())));
            }
            Event::PermissionResolved { target_session_id, call_id, reason } => {
                out.push(self.make("permission.resolved", Some(serde_json::json!({
                    "sessionId": target_session_id, "callId": call_id, "reason": reason,
                }).to_string())));
            }
            Event::SandboxRequest { target_session_id, call_id, operation, args } => {
                out.push(self.make("sandbox.request", Some(serde_json::json!({
                    "sessionId": target_session_id, "callId": call_id, "operation": operation, "args": args,
                }).to_string())));
            }
            Event::ToolStarted { call_id, tool, args, parent_call_id } => {
                if self.detailed {
                    out.push(self.make("tool.started", Some(serde_json::json!({
                        "callId": call_id, "tool": tool, "args": args, "parentCallId": parent_call_id,
                    }).to_string())));
                }
                let path = args.get("path").and_then(|v| v.as_str());
                let summary = path.or_else(|| args.get("command").and_then(|v| v.as_str()))
                    .or_else(|| args.get("pattern").and_then(|v| v.as_str())).map(str::to_owned);
                let mut ev = self.make("tool", summary);
                ev.tool = Some(tool.clone());
                ev.call_id = Some(call_id.clone());
                ev.parent_call_id = parent_call_id;
                out.push(ev);
                if self.write_tools.contains(&tool) {
                    if let Some(path) = path {
                        self.tools.insert(call_id, self.cwd.join(path).to_string_lossy().replace('\\', "/"));
                    }
                }
            }
            Event::ToolCompleted { call_id, tool, is_error, .. } => {
                if self.detailed {
                    out.push(self.make("tool.completed", Some(serde_json::json!({
                        "callId": call_id, "tool": tool, "isError": is_error,
                    }).to_string())));
                }
                let path = self.tools.remove(&call_id);
                if is_error {
                    // A tool failure is visible, but the agent may recover and finish successfully.
                    out.push(self.make("error", Some(format!("ToolFailed: {tool} ({call_id})"))));
                } else if let Some(path) = path {
                    if is_displayable_artifact(&path) && packaged_project_root(std::path::Path::new(&path)).is_none() {
                        out.push(self.make("artifact", Some(path)));
                    }
                }
            }
            Event::Error { code, message } => {
                if crate::provider::pi_failure_counts(&code) { self.failure_code = Some(code.clone()); }
                self.error_seen = true;
                out.push(self.make("error", Some(format!("{code}: {message}"))));
            }
            Event::Completed { status } => {
                if let Some(provider) = self.provider.take() {
                    let code = self.failure_code.take();
                    let success = status == TurnStatus::Completed;
                    let failed = status == TurnStatus::Error;
                    std::thread::spawn(move || crate::provider::pi_outcome(&provider, success, failed, code.as_deref()));
                }
                if matches!(status, TurnStatus::Error | TurnStatus::Timeout) && !self.error_seen {
                    out.push(self.make("error", Some(format!("AgentdTurnFailed: {status:?}"))));
                }
                self.done = true;
                let status = self.detailed.then(|| format!("{status:?}"));
                out.push(self.make("done", status));
            }
            // The current frontend contract has no reasoning/usage event. Future
            // support should extend it with optional fields, preserving existing consumers.
            Event::Reasoning { .. } | Event::Usage { .. } => {}
            Event::Started => {}
            Event::Text { .. } => {}
        }
        out
    }
}

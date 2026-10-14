//! Existing CLI spawn, parsing, prompt assembly and pool, moved without rewriting.
pub mod pool;
pub(crate) mod pipeline;
mod headless;
use super::*;
use parking_lot::Mutex;
use std::collections::HashMap;
use crate::chat::ChatStreamEvent;
use crate::runtime::procs::{CHILDREN, kill_tree};
#[derive(Clone)]
struct Capture { stream: Arc<stream::Mailbox>, session: String }
static STREAMS: once_cell::sync::Lazy<Mutex<HashMap<String, Capture>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(HashMap::new()));

pub(crate) fn capture(event: &ChatStreamEvent) -> bool {
    let sender = STREAMS.lock().get(&event.req_id).map(|entry| entry.stream.clone());
    if let Some(sender) = sender {
        if event.kind == "done" {
            sender.finish(None, event.clone());
            STREAMS.lock().remove(&event.req_id);
        } else { sender.push_blocking(event.clone()); }
        true
    } else { false }
}
pub struct ClaudeCliBackend;
impl AgentBackend for ClaudeCliBackend {
    fn start(&self, mut input: TurnRequest) -> anyhow::Result<EventStream> {
        if input.legacy.is_none() {
            anyhow::ensure!(matches!(input.permission, PermissionMode::Deny) && input.model.is_none()
                && input.attachments.is_empty(), BackendError::UnsupportedInput("legacy no-UI entry supports read-only text with its existing configured model"));
        }
        let config = config::Config::load()?.agentd;
        let (sender, stream) = stream::Mailbox::channel(config.channel_capacity, config.max_line_bytes);
        {
            let mut streams = STREAMS.lock();
            anyhow::ensure!(!streams.contains_key(&input.req_id), "ClaudeCliDuplicateRequest");
            streams.insert(input.req_id.clone(), Capture { stream: sender.clone(), session: input.session_id.clone() });
        }
        std::thread::spawn(move || {
            let req = input.req_id.clone();
            let conv = input.conversation_id.clone();
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                if let Some((app, args)) = input.legacy.take() {
                    pipeline::send(&app, &req, args)
                } else {
                    let callback = |kind: &str, text: &str| {
                        let mut e = event(&req, conv.as_deref(), kind, Some(text.into()));
                        if kind == "tool" { e.tool = e.text.take(); }
                        sender.push_blocking(e);
                    };
                    let result = headless::run_readonly(&input.cwd, &input.prompt, callback, input.timeout, &req);
                    sender.finish(result.err().map(|e| event(&req, conv.as_deref(), "error", Some(e))),
                        event(&req, conv.as_deref(), "done", None));
                    STREAMS.lock().remove(&req);
                    Ok(())
                }
            }));
            let error = match outcome {
                Ok(Ok(())) => None,
                Ok(Err(e)) => Some(e),
                Err(_) => Some("ClaudeCliBackend: background preparation panicked".into()),
            };
            if let Some(error) = error {
                sender.finish(Some(event(&req, conv.as_deref(), "error", Some(error))), event(&req, conv.as_deref(), "done", None));
                STREAMS.lock().remove(&req);
                CHILDREN.take_cancel(&req);
            }
        });
        Ok(stream)
    }
    fn cancel(&self, req_id: &str) -> anyhow::Result<()> {
        if !pool::cancel_by_req(req_id) {
            if let Some(mut child) = CHILDREN.remove(req_id) {
                kill_tree(child.id());
                let _ = child.kill();
                let _ = child.wait();
            } else { CHILDREN.mark_cancel(req_id); }
        }
        Ok(())
    }
    fn close(&self, session: &str) -> anyhow::Result<()> {
        pool::kill_conv(session);
        let requests: Vec<_> = STREAMS.lock().iter().filter(|(_, entry)| entry.session == session)
            .map(|(req, _)| req.clone()).collect();
        for req in requests { self.cancel(&req)?; }
        Ok(())
    }
    fn models(&self) -> anyhow::Result<serde_json::Value> {
        let mut models = Vec::new();
        for model in [pipeline::model_for_mode(false), pipeline::model_for_mode(true), std::env::var("ANTHROPIC_MODEL").ok()].into_iter().flatten() {
            if !model.is_empty() && !models.contains(&model) { models.push(model); }
        }
        Ok(serde_json::json!({"models": models, "source": "configured-cli-models"}))
    }
    fn status(&self) -> BackendStatus { let mut s = BackendStatus::new("claude-cli"); s.healthy = crate::doctor::resolve_claude_exe().is_some(); s }
    fn is_running(&self, req: &str) -> bool {
        STREAMS.lock().contains_key(req) || pool::is_active_req(req) || CHILDREN.lock().contains_key(req)
    }
    fn shutdown(&self) {
        pool::kill_all();
        let requests: Vec<_> = STREAMS.lock().keys().cloned().collect();
        for req in requests { let _ = self.cancel(&req); }
    }
}

#[cfg(test)]
mod tests;

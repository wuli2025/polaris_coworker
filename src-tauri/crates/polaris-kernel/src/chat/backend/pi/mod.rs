//! Shared daemon adapter. Per-account fails closed until the shell can supply
//! authenticated OS isolation; different working directories are not a sandbox.
mod client;
mod experts;
mod process;
pub mod protocol;
mod routes;
mod sessions;
mod input;
#[cfg(test)] mod extension_tests;
#[cfg(test)] mod wiki_tests;
mod supervisor;
mod translate;
#[cfg(windows)] mod windows_job;
#[cfg(test)] mod tests;
#[cfg(test)] mod live_input_tests;
use super::*;
use client::{Client, Control};
use protocol::{Command, Empty, Prompt};
use parking_lot::Mutex;
use std::{collections::HashSet, sync::mpsc};
use serde_json::Value;

pub struct PiBackend {
    config: config::Agentd,
    client: once_cell::sync::OnceCell<Arc<Client>>,
    preparing: Arc<Mutex<HashSet<String>>>,
    cancelled: Arc<Mutex<HashSet<String>>>,
    expert_lock: Mutex<()>,
    provider_config: Mutex<Option<(Option<u32>, Value)>>,
}
impl PiBackend {
    pub fn new(config: config::Agentd) -> anyhow::Result<Self> {
        config::Config { agent: config::Agent::default(), agentd: config.clone() }.validate()?;
        let backend = Self { config, client: once_cell::sync::OnceCell::new(),
            provider_config: Mutex::new(None), expert_lock: Mutex::new(()),
            preparing: Arc::new(Mutex::new(HashSet::new())), cancelled: Arc::new(Mutex::new(HashSet::new())) };
        // Start supervision without waiting for daemon I/O or expert restoration.
        if backend.config.isolation == "shared" { backend.client()?; }
        Ok(backend)
    }
    fn client(&self) -> anyhow::Result<Arc<Client>> {
        if self.config.isolation == "per-account" { return Err(BackendError::AccountContextRequired.into()); }
        self.client.get_or_try_init(|| Client::new(self.config.clone())).cloned()
    }
    /// Configure a mapped idle session through the protocol's typed public options.
    pub fn configure_session(&self, key: &str, options: protocol::SessionOptions) -> anyhow::Result<Value> {
        sessions::configure(self.client()?.as_ref(), key, options)
    }
    /// Process-wide configuration; close every session reported by the daemon.
    pub fn apply_config(&self, config: Option<Value>, credentials: Option<Value>) -> anyhow::Result<Value> {
        let client = self.client()?;
        // agentd reopens idle sessions with their existing SessionManager and defers
        // busy sessions until completion; never abort an in-flight provider switch.
        let result = client.call(Command::Apply(protocol::Apply { config, credentials }))?;
        Ok(result)
    }
}
impl AgentBackend for PiBackend {
    fn experts_enabled(&self, enabled: Option<bool>) -> anyhow::Result<bool> { self.expert_state(enabled) }
    fn auth(&self, operation: &str, provider: &str, input: Option<String>) -> anyhow::Result<Value> {
        self.client()?.call(Command::Auth(protocol::Auth { operation: operation.into(), provider: provider.into(), input }))
    }
    fn provider_apply(&self, id: &str) -> anyhow::Result<Value> {
        let (generation, selected) = crate::provider::selection::snapshot();
        if selected != id { return Ok(serde_json::json!({"applied":false,"superseded":true})); }
        let client = self.client()?;
        // Serialize deliveries, independently of provider state/queries. Check the
        // ticket both before sending and after every potentially slow daemon call.
        let mut last = self.provider_config.lock();
        let current = || crate::provider::selection::current(generation);
        let superseded = || serde_json::json!({"applied":false,"superseded":true});
        if !current() { return Ok(superseded()); }
        let catalog = client.call(Command::Catalog(Empty {}));
        if !current() { return Ok(superseded()); }
        let catalog = protocol::Catalog::parse(catalog?)?;
        let config = crate::provider::pi_translate::config(id, &serde_json::to_value(catalog)?).map_err(anyhow::Error::msg)?;
        let pid = client.status.lock().pid;
        if !current() { return Ok(superseded()); }
        if last.as_ref() == Some(&(pid, config.clone())) { return Ok(serde_json::json!({"applied":true})); }
        let result = self.apply_config(Some(config.clone()), None);
        if !current() { *last = None; return Ok(superseded()); }
        let result = result?;
        *last = Some((pid, config));
        Ok(result)
    }

    fn permission_respond(&self, response: protocol::PermissionResponse) -> anyhow::Result<()> {
        self.client()?.call(Command::PermissionRespond(response)).map(|_| ())
    }
    fn sandbox_respond(&self, response: protocol::SandboxResponse) -> anyhow::Result<()> {
        self.client()?.call(Command::SandboxRespond(response)).map(|_| ())
    }
    fn start(&self, input: TurnRequest) -> anyhow::Result<EventStream> {
        self.start_with_tools(input, ToolOptions::default())
    }
    fn start_with_tools(&self, input: TurnRequest, options: ToolOptions) -> anyhow::Result<EventStream> {
        if self.config.isolation == "per-account" {
            return Err(if input.account_id.is_some() { BackendError::UnsupportedIsolation } else { BackendError::AccountContextRequired }.into());
        }
        anyhow::ensure!(!input.prompt.trim().is_empty(), "AgentdEmptyPrompt");
        if crate::provider::credential_file::provider_store_path().is_some() {
            loop {
                let (_, id) = crate::provider::selection::snapshot();
                if self.provider_apply(&id)?["superseded"] != true { break; }
            }
        }
        let client = self.client()?;
        if client.status.lock().circuit_open { return Err(BackendError::CircuitOpen.into()); }
        let (sender, stream) = stream::Mailbox::channel(self.config.channel_capacity, self.config.max_line_bytes);
        let preparing = self.preparing.clone();
        let cancelled = self.cancelled.clone();
        anyhow::ensure!(preparing.lock().insert(input.req_id.clone()), "AgentdDuplicateRequest");
        let key = format!("session:{}", input.session_id);
        if !preparing.lock().insert(key.clone()) {
            preparing.lock().remove(&input.req_id);
            anyhow::bail!("AgentdSessionBusy");
        }
        std::thread::spawn(move || {
            let req = input.req_id.clone();
            let conv = input.conversation_id.clone();
            let mut registered = false;
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> anyhow::Result<()> {
                sender.push_blocking(event(&req, conv.as_deref(), "meta",
                    Some(crate::chat::prompt::estimate_tokens(&input.prompt).to_string())));
                if cancelled.lock().remove(&req) { sender.finish(None, event(&req, conv.as_deref(), "done", None)); return Ok(()); }
                let detailed_events = options.detailed_events;
                let info = sessions::open(&client, &input, options)?;
                if matches!(input.permission, PermissionMode::Deny) {
                    anyhow::ensure!(info.tools.iter().all(|t| client.config.readonly_tools.contains(t)),
                        BackendError::UnsupportedInput("read-only/manual request cannot use a daemon with write or execution tools"));
                }
                let (tx, rx) = mpsc::sync_channel(1);
                // Control::Register.timeout is the hard limit; zero means unlimited.
                let configured_max = Duration::from_millis(client.config.turn_max_ms);
                // Preserve an explicit zero request budget as immediate expiry,
                // rather than confusing it with the configured unlimited sentinel.
                let timeout = match input.timeout.map(|limit| limit.max(Duration::from_nanos(1))) {
                    Some(limit) if !configured_max.is_zero() => limit.min(configured_max),
                    Some(limit) => limit,
                    None => configured_max,
                };
                client.send(Control::Register { session: info.id.clone(), req: req.clone(),
                    translator: translate::Translator::new(req.clone(), conv.clone(), input.cwd.clone(), &client.config)
                        .with_details(detailed_events)
                        .with_provider(info.model.get("provider").and_then(Value::as_str).unwrap_or_default()),
                    stream: sender.clone(), timeout, reservation: (preparing.clone(), key.clone()), reply: tx })?;
                client.wait(rx)?;
                registered = true;
                let result = client.call(Command::Prompt(Prompt {
                    session_id: info.id.clone(), text: input.prompt, attachments: input.attachments,
                }));
                eprintln!("{}", serde_json::json!({"component":"agent.backend", "backend":"pi",
                    "reqId":req, "conversationId":conv, "sessionId":info.id, "phase":"prompt", "ok":result.is_ok()}));
                let result = result?;
                let turn = result.get("turnId").and_then(Value::as_str).ok_or_else(|| anyhow::anyhow!("AgentdMissingTurnId"))?;
                let (tx, rx) = mpsc::sync_channel(1);
                client.send(Control::Bind(req.clone(), turn.into(), tx))?;
                client.wait(rx)?;
                if cancelled.lock().remove(&req) { client.cancel(&req)?; }
                Ok(())
            }));
            let error = match result { Ok(Ok(())) => None, Ok(Err(e)) => Some(e.to_string()), Err(_) => Some("AgentdPreparationPanic".into()) };
            if let Some(error) = error {
                if registered {
                    let _ = client.send(Control::Fail(req.clone(), error));
                } else {
                    sender.finish(Some(event(&req, conv.as_deref(), "error", Some(error))), event(&req, conv.as_deref(), "done", None));
                }
            }
            preparing.lock().remove(&req);
            if !registered { preparing.lock().remove(&key); }
            cancelled.lock().remove(&req);
        });
        Ok(stream)
    }
    fn live_input(&self, req: &str, text: &str, follow_up: bool) -> anyhow::Result<Value> {
        let client = self.client()?;
        let (tx, rx) = mpsc::sync_channel(1);
        client.send(Control::Input { req: req.into(), text: text.into(), follow_up, reply: tx })?;
        client.wait(rx)
    }
    fn cancel(&self, req: &str) -> anyhow::Result<()> {
        if self.preparing.lock().contains(req) { self.cancelled.lock().insert(req.into()); }
        if let Some(client) = self.client.get() { client.cancel(req)?; }
        Ok(())
    }
    fn close(&self, session: &str) -> anyhow::Result<()> {
        if let Some(client) = self.client.get() { sessions::close(client, session)?; }
        Ok(())
    }
    fn models(&self) -> anyhow::Result<Value> { self.client()?.call(Command::Models(Empty {})) }
    fn status(&self) -> BackendStatus {
        self.client.get().map(|c| c.status.lock().clone()).unwrap_or_else(|| BackendStatus::new("pi"))
    }
    fn is_running(&self, req: &str) -> bool {
        self.preparing.lock().contains(req) || self.client.get().is_some_and(|c| c.active.lock().contains_key(req))
    }
    fn shutdown(&self) { if let Some(client) = self.client.get() { client.stop(); } }
}
impl Drop for PiBackend { fn drop(&mut self) { self.shutdown(); } }

#[cfg(test)]
mod test_support;

use super::{protocol::Command, supervisor, translate::Translator};
use super::super::{config::Agentd, stream::Mailbox, BackendStatus};
use std::{sync::{Arc, mpsc}, time::{Duration, Instant}};
use parking_lot::Mutex;
use serde_json::Value;
pub type Reply = mpsc::SyncSender<Result<Value, String>>;
pub enum Control {
    Request(Command, Reply),
    Register { session: String, req: String, translator: Translator, stream: Arc<Mailbox>, timeout: Duration, reservation: (Arc<Mutex<std::collections::HashSet<String>>>, String), reply: Reply },
    Cancel(String, Reply),
    Bind(String, String, Reply),
    Fail(String, String),
    Stop,
}
pub struct Client {
    pub config: Agentd,
    tx: tokio::sync::mpsc::Sender<Control>,
    pub status: Arc<Mutex<BackendStatus>>,
    pub active: Arc<Mutex<std::collections::HashMap<String, String>>>,
}
impl Client {
    pub fn new(config: Agentd) -> anyhow::Result<Arc<Self>> {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
        let (tx, rx) = tokio::sync::mpsc::channel(config.channel_capacity);
        let client = Arc::new(Self { config: config.clone(), tx,
            status: Arc::new(Mutex::new(BackendStatus::new("pi"))), active: Arc::new(Mutex::new(Default::default())) });
        let status = client.status.clone();
        let active = client.active.clone();
        std::thread::Builder::new().name("agentd-supervisor".into()).spawn(move || {
            runtime.block_on(supervisor::run(config, rx, status, active));
        })?;
        Ok(client)
    }
    pub fn send(&self, mut command: Control) -> anyhow::Result<()> {
        let deadline = Instant::now() + Duration::from_millis(self.config.request_timeout_ms);
        loop {
            match self.tx.try_send(command) {
                Ok(()) => return Ok(()),
                Err(tokio::sync::mpsc::error::TrySendError::Full(c)) => command = c,
                Err(_) => anyhow::bail!("AgentdSupervisorStopped"),
            }
            anyhow::ensure!(Instant::now() < deadline, "AgentdControlTimeout");
            std::thread::sleep(Duration::from_millis(self.config.control_poll_ms));
        }
    }
    pub fn call(&self, command: Command) -> anyhow::Result<Value> {
        let (tx, rx) = mpsc::sync_channel(1);
        self.send(Control::Request(command, tx))?;
        self.wait(rx)
    }
    pub fn wait(&self, rx: mpsc::Receiver<Result<Value, String>>) -> anyhow::Result<Value> {
        // Budget queue/serialization separately from the actor's request deadline,
        // so its specific write/response error wins over the caller's fallback.
        let timeout = self.config.request_timeout_ms.saturating_mul(2)
            .saturating_add(self.config.supervisor_tick_ms.saturating_mul(2));
        rx.recv_timeout(Duration::from_millis(timeout)).map_err(|error| {
            let status = self.status.lock();
            let message = if !status.healthy { status.last_error.clone() } else { None };
            anyhow::anyhow!(message.unwrap_or_else(|| match error {
                mpsc::RecvTimeoutError::Disconnected => "AgentdSupervisorStopped".into(),
                mpsc::RecvTimeoutError::Timeout => "AgentdRequestTimeout".into(),
            }))
        })?.map_err(anyhow::Error::msg)
    }
    pub fn cancel(&self, req: &str) -> anyhow::Result<()> {
        let (tx, rx) = mpsc::sync_channel(1);
        self.send(Control::Cancel(req.into(), tx))?;
        self.wait(rx).map(|_| ())
    }
    pub fn stop(&self) { let _ = self.send(Control::Stop); }
}
impl Drop for Client { fn drop(&mut self) { let _ = self.tx.try_send(Control::Stop); } }

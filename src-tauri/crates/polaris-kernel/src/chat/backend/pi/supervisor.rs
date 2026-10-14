use super::{client::{Control, Reply}, process::Process, protocol::{Command, Empty, Frame, Id, Request, Session}, routes::Route};
use super::super::{config::Agentd, BackendStatus, BackendError};
use parking_lot::Mutex;
use std::{collections::HashMap, sync::Arc, time::{Duration, Instant}};
use tokio::sync::mpsc;
use serde_json::{Value, json};
struct Pending { reply: Option<Reply>, since: Instant }
struct State {
    config: Agentd,
    status: Arc<Mutex<BackendStatus>>,
    active: Arc<Mutex<HashMap<String, String>>>,
    routes: HashMap<String, Route>,
    pending: HashMap<Id, Pending>,
    counter: u64,
    failures: u32,
    next_start: Instant,
    last_ping: Instant,
    started: Instant,
    last_used: Instant,
    stopping: bool,
}
impl State {
    fn request(&mut self, process: &Process, command: Command, reply: Option<Reply>) -> anyhow::Result<()> {
        self.counter += 1;
        let id = Id::Text(format!("rust-{}", self.counter));
        process.write(&Request { id: id.clone(), command }, self.config.max_line_bytes)?;
        self.pending.insert(id, Pending { reply, since: Instant::now() });
        Ok(())
    }
    fn fail(&mut self, error: String) {
        self.failures = self.failures.saturating_add(1);
        let circuit = self.failures >= self.config.failure_threshold;
        let error = if circuit { format!("{}: {error}", BackendError::CircuitOpen) } else { error };
        for route in self.routes.values_mut() { route.fail(&error); }
        let mut status = self.status.lock();
        status.healthy = false;
        status.pid = None;
        status.circuit_open = circuit;
        eprintln!("{}", json!({"component":"agent.backend", "backend":"pi", "phase":"failure", "error":error}));
        status.last_error = Some(error.clone());
        drop(status);
        for (_, pending) in self.pending.drain() {
            if let Some(reply) = pending.reply { let _ = reply.send(Err(error.clone())); }
        }
        let backoff = self.config.restart_base_ms.saturating_mul(1_u64 << self.failures.saturating_sub(1).min(30))
            .min(self.config.restart_max_ms);
        let mut bytes = [0_u8; 8];
        let _ = getrandom::getrandom(&mut bytes);
        let jitter = u64::from_le_bytes(bytes) % self.config.restart_jitter_ms.saturating_add(1).max(1);
        self.next_start = Instant::now() + Duration::from_millis(backoff.saturating_add(jitter).min(self.config.restart_max_ms));
    }
    fn frame(&mut self, frame: Frame) -> anyhow::Result<()> {
        match frame {
            Frame::Response { id, ok, result, error } => {
                let id = id.ok_or_else(|| anyhow::anyhow!("AgentdProtocolError: uncorrelated response"))?;
                if let Some(pending) = self.pending.remove(&id) {
                    let result = if ok { Ok(result.unwrap_or(Value::Null)) }
                        else { Err(error.map(|e| format!("{}: {}", e.code, e.message)).unwrap_or_else(|| "AgentdResponseError".into())) };
                    if let Some(reply) = pending.reply { let _ = reply.send(result); }
                    else {
                        let value = result.map_err(anyhow::Error::msg)?;
                        anyhow::ensure!(value.get("status").and_then(Value::as_str) == Some("ok"), "AgentdPingFailed");
                        self.status.lock().healthy = true;
                    }
                }
            }
            Frame::Event { session_id, turn_id, event } => {
                if let Some(route) = self.routes.get_mut(&session_id) { route.accept(turn_id, event)?; }
                // Events after a locally finalized abort are intentionally ignored.
            }
        }
        Ok(())
    }
    fn control(&mut self, command: Control, process: Option<&Process>) -> anyhow::Result<()> {
        self.last_used = Instant::now();
        match command {
            Control::Stop => { self.stopping = true; }
            #[cfg(test)]
            Control::TestExit(_) | Control::TestClose => unreachable!("handled by run"),
            Control::Fail(req, message) => {
                for route in self.routes.values_mut().filter(|r| r.req == req) { route.fail(&message); }
            }
            Control::Register { session, req, translator, stream, timeout, reservation, reply } => {
                if self.routes.contains_key(&session) {
                    let _ = reply.send(Err("AgentdSessionBusy".into()));
                } else if process.is_none() {
                    let _ = reply.send(Err(self.unavailable()));
                } else {
                    self.active.lock().insert(req.clone(), session.clone());
                    let now = Instant::now();
                    self.routes.insert(session.clone(), Route { req, session, active: self.active.clone(), acknowledged: false, started: false, reservation, turn: None, translator, stream,
                        registered_at: now, last_event_at: now,
                        idle_timeout: Duration::from_millis(self.config.turn_idle_timeout_ms),
                        max_duration: timeout, abort_deadline: None,
                        pending: Default::default(), terminal: None, first_error: None });
                    let _ = reply.send(Ok(json!({})));
                }
            }
            Control::Request(command, reply) => {
                if let Some(process) = process {
                    if let Err(e) = self.request(process, command, Some(reply.clone())) {
                        let _ = reply.send(Err(e.to_string()));
                        return Err(e);
                    }
                } else { let _ = reply.send(Err(self.unavailable())); }
            }
            Control::Bind(req, turn, reply) => {
                let route = self.routes.values_mut().find(|r| r.req == req);
                let result = match route {
                    Some(route) if route.turn.as_ref().is_none_or(|id| id == &turn) => {
                        route.turn = Some(turn);
                        route.acknowledged = true;
                        Ok(json!({}))
                    }
                    Some(route) => { route.fail("AgentdTurnMismatch"); Err("AgentdTurnMismatch".into()) }
                    None => Err("AgentdTurnMissing".into()),
                };
                let _ = reply.send(result);
            }
            Control::Cancel(req, reply) => {
                let session = self.active.lock().get(&req).cloned();
                if let Some(session) = session {
                    if let Some(route) = self.routes.get_mut(&session) {
                        route.abort_deadline = Some(Instant::now() + Duration::from_millis(self.config.abort_timeout_ms));
                    }
                    if let Some(process) = process {
                        // Abort response may arrive after turn.completed. Ignore only its result,
                        // using an ordinary reply prevents it being mistaken for a health ping.
                        let (tx, _rx) = std::sync::mpsc::sync_channel(1);
                        self.request(process, Command::Abort(Session { session_id: session }), Some(tx))?;
                    }
                }
                let _ = reply.send(Ok(json!({"accepted":true})));
            }
        }
        Ok(())
    }
    fn unavailable(&self) -> String {
        let status = self.status.lock();
        if status.circuit_open { BackendError::CircuitOpen.to_string() }
        else { status.last_error.clone().unwrap_or_else(|| "AgentdRestarting".into()) }
    }
    fn drain(&mut self) -> bool {
        let mut ready = true;
        self.routes.retain(|_, route| {
            route.tick();
            ready &= route.drain();
            if route.stream.is_finished() { self.active.lock().remove(&route.req); false } else { true }
        });
        ready
    }
    fn deadlines(&mut self, process: Option<&Process>) -> anyhow::Result<()> {
        if let Some(error) = process.and_then(Process::write_error) { anyhow::bail!(error); }
        for route in self.routes.values_mut() {
            if route.abort_deadline.is_some_and(|d| Instant::now() >= d) {
                route.aborted();
                anyhow::bail!("AgentdAbortTimeout: daemon restarted to stop the cancelled turn");
            }
            if let Some(error) = route.timeout_error(Instant::now()) {
                route.fail(error);
                anyhow::bail!(error);
            }
        }
        if self.pending.values().any(|p| p.since.elapsed().as_millis() >= self.config.request_timeout_ms as u128) {
            if process.is_some_and(Process::writes_pending) { anyhow::bail!(super::process::WRITER_BACKPRESSURE); }
            anyhow::bail!("AgentdRequestTimeout");
        }
        Ok(())
    }
}
// Also runs when the supervisor future is dropped or its thread unwinds.
impl Drop for State {
    fn drop(&mut self) {
        {
            let mut status = self.status.lock();
            status.healthy = false;
            status.pid = None;
            if !self.stopping || !self.routes.is_empty() {
                status.last_error = Some(super::routes::SUPERVISOR_EXITED.into());
            }
        }
        // Publish failure before waking stream consumers and control callers.
        self.routes.clear();
        self.active.lock().clear();
    }
}
pub async fn run(config: Agentd, mut rx: mpsc::Receiver<Control>, status: Arc<Mutex<BackendStatus>>,
    active: Arc<Mutex<HashMap<String, String>>>) {
    let now = Instant::now();
    let mut s = State { config, status, active, routes: HashMap::new(), pending: HashMap::new(),
        counter: 0, failures: 0, next_start: now, last_ping: now, started: now, last_used: now, stopping: false };
    let mut process: Option<Process> = None;
    let mut ever_started = false;
    let mut idle = false;
    let mut ticker = tokio::time::interval(Duration::from_millis(s.config.supervisor_tick_ms));
    loop {
        if !idle && process.is_none() && !s.status.lock().circuit_open && Instant::now() >= s.next_start {
            match Process::spawn(&s.config) {
                Ok(p) => {
                    { let mut status = s.status.lock(); status.pid = Some(p.pid); if ever_started { status.restarts += 1; } }
                    ever_started = true;
                    s.started = Instant::now();
                    s.last_ping = Instant::now();
                    if let Err(e) = s.request(&p, Command::Ping(Empty {}), None) { s.fail(e.to_string()); }
                    process = Some(p);
                }
                Err(e) => s.fail(e.to_string()),
            }
        }
        let ready = s.drain();
        let outcome: anyhow::Result<()> = tokio::select! {
            command = rx.recv() => {
                match command {
                    #[cfg(test)]
                    Some(Control::TestExit(panic)) => {
                        assert!(!panic, "injected supervisor panic");
                        return;
                    }
                    #[cfg(test)]
                    Some(Control::TestClose) => { rx.close(); Ok(()) }
                    Some(command) => {
                        if idle {
                            idle = false;
                            match Process::spawn(&s.config) {
                                Ok(p) => {
                                    { let mut status = s.status.lock(); status.pid = Some(p.pid); status.restarts += 1; }
                                    s.started = Instant::now();
                                    s.last_ping = Instant::now();
                                    if let Err(error) = s.request(&p, Command::Ping(Empty {}), None) { s.fail(error.to_string()); }
                                    process = Some(p);
                                }
                                Err(e) => s.fail(e.to_string()),
                            }
                        }
                        s.control(command, process.as_ref())
                    }
                    None => break,
                }
            }
            frame = async { match &mut process { Some(p) => p.incoming.recv().await, None => std::future::pending().await } }, if ready => {
                match frame { Some(Ok(frame)) => s.frame(frame), Some(Err(e)) => Err(e), None => Err(anyhow::anyhow!("AgentdEof")) }
            }
            _ = ticker.tick() => {
                let deadlines = s.deadlines(process.as_ref());
                if let Some(p) = &mut process {
                    if p.exited().unwrap_or(true) { Err(anyhow::anyhow!("AgentdExited")) }
                    else if deadlines.is_err() { deadlines }
                    else if ready && s.last_ping.elapsed().as_millis() >= s.config.ping_interval_ms as u128 {
                        s.last_ping = Instant::now();
                        s.request(p, Command::Ping(Empty {}), None)
                    } else {
                        if s.status.lock().healthy && s.started.elapsed().as_millis() >= s.config.stable_reset_ms as u128 { s.failures = 0; }
                        Ok(())
                    }
                } else { deadlines }
            }
        };
        if let Err(error) = outcome {
            s.fail(error.to_string());
            if let Some(mut p) = process.take() { p.stop().await; }
        }
        if s.stopping {
            for route in s.routes.values_mut() { route.fail("AgentdShutdown"); }
            if let Some(mut p) = process.take() { p.stop().await; }
            // Delivery is cancellable by dropping the receiver; keep draining retained events.
            while !s.routes.is_empty() { s.drain(); ticker.tick().await; }
            break;
        }
        if process.is_some() && s.routes.is_empty() && s.pending.is_empty()
            && s.last_used.elapsed().as_millis() >= s.config.idle_ms as u128 {
            if let Some(mut p) = process.take() { p.stop().await; }
            let mut status = s.status.lock(); status.pid = None; status.healthy = false;
            idle = true;
        }
    }
    let mut status = s.status.lock(); status.pid = None; status.healthy = false;
}

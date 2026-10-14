use super::{translate::Translator, protocol::{Event, TurnStatus}};
use super::super::stream::Mailbox;
use crate::chat::ChatStreamEvent;
use std::{collections::VecDeque, sync::Arc, time::Instant};
pub struct Route {
    pub req: String,
    pub session: String,
    pub active: Arc<parking_lot::Mutex<std::collections::HashMap<String, String>>>,
    pub acknowledged: bool,
    pub started: bool,
    pub reservation: (Arc<parking_lot::Mutex<std::collections::HashSet<String>>>, String),
    pub turn: Option<String>,
    pub translator: Translator,
    pub stream: Arc<Mailbox>,
    pub deadline: Instant,
    pub abort_deadline: Option<Instant>,
    pub pending: VecDeque<ChatStreamEvent>,
    pub terminal: Option<(Option<ChatStreamEvent>, ChatStreamEvent)>,
    pub first_error: Option<String>,
}
impl Route {
    pub fn accept(&mut self, turn: String, event: Event) -> anyhow::Result<()> {
        if self.terminal.is_some() || self.stream.is_finished() { return Ok(()); }
        if !self.started {
            anyhow::ensure!(matches!(event, Event::Started), "AgentdTurnOrder: expected turn.started");
            self.started = true;
            if self.turn.is_none() { self.turn = Some(turn.clone()); }
        }
        anyhow::ensure!(self.turn.as_deref() == Some(&turn), "AgentdTurnMismatch");
        let fatal = matches!(event, Event::Error { .. } | Event::Completed { .. });
        let events = self.translator.accept(event);
        if fatal && self.first_error.is_none() {
            self.first_error = events.iter().find(|e| e.kind == "error").and_then(|e| e.text.clone());
        }
        self.enqueue(events);
        Ok(())
    }
    pub fn tick(&mut self) {
        if self.terminal.is_none() { let events = self.translator.tick(); self.enqueue(events); }
    }
    fn enqueue(&mut self, events: Vec<ChatStreamEvent>) {
        for event in events {
            if event.kind == "done" { self.terminal = Some((None, event)); }
            else { self.pending.push_back(event); }
        }
    }
    pub fn fail(&mut self, message: &str) {
        if self.stream.is_finished() { return; }
        self.acknowledged = true;
        let error = if self.first_error.is_some() {
            eprintln!("{}", serde_json::json!({"component":"agent.backend", "backend":"pi",
                "reqId":self.req, "phase":"secondary-error", "error":message, "firstError":self.first_error}));
            None
        } else {
            self.first_error = Some(message.into());
            Some(self.translator.make("error", Some(message.into())))
        };
        if let Some((existing, _)) = &mut self.terminal {
            if existing.is_none() { *existing = error; }
            return;
        }
        let events = self.translator.flush();
        self.enqueue(events);
        self.terminal = Some((error, self.translator.make("done", None)));
    }
    pub fn aborted(&mut self) {
        if self.terminal.is_none() {
            let events = self.translator.accept(Event::Completed { status: TurnStatus::Aborted });
            self.enqueue(events);
        }
    }
    pub fn drain(&mut self) -> bool {
        while let Some(event) = self.pending.pop_front() {
            if let Err(event) = self.stream.try_push(event) {
                self.pending.push_front(event);
                return false;
            }
        }
        if self.acknowledged {
            if let Some((error, done)) = self.terminal.take() {
                eprintln!("{}", serde_json::json!({"component":"agent.backend", "backend":"pi",
                    "reqId":self.req, "conversationId":done.conversation_id, "sessionId":self.session,
                    "phase":"done", "error":error.is_some()}));
                self.reservation.0.lock().remove(&self.reservation.1);
                self.active.lock().remove(&self.req);
                self.stream.finish(error, done);
            }
        }
        true
    }
}

impl Drop for Route {
    fn drop(&mut self) {
        self.reservation.0.lock().remove(&self.reservation.1);
        self.active.lock().remove(&self.req);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::super::config::Agentd;
    fn route() -> (Route, super::super::super::EventStream) {
        let (stream, receiver) = Mailbox::channel(4, 4096);
        let active = Arc::new(parking_lot::Mutex::new(std::collections::HashMap::from([("r".into(), "s".into())])));
        let reservations = Arc::new(parking_lot::Mutex::new(std::collections::HashSet::from(["s".into()])));
        (Route { req: "r".into(), session: "s".into(), active, acknowledged: true, started: false,
            reservation: (reservations, "s".into()), turn: None,
            translator: Translator::new("r".into(), None, "/tmp".into(), &Agentd::default()), stream,
            deadline: Instant::now(), abort_deadline: None, pending: VecDeque::new(), terminal: None, first_error: None }, receiver)
    }
    #[test]
    fn dropping_undrained_route_releases_active_and_reservation() {
        let (route, _receiver) = route();
        let active = route.active.clone();
        let reservations = route.reservation.0.clone();
        drop(route);
        assert!(active.lock().is_empty());
        assert!(reservations.lock().is_empty());
    }
    #[test]
    fn first_failure_survives_abort_timeout() {
        for daemon_event in [false, true] {
            let (mut route, receiver) = route();
            if daemon_event {
                route.accept("t".into(), Event::Started).unwrap();
                route.accept("t".into(), Event::Error { code: "MODEL_ERROR".into(), message: "original".into() }).unwrap();
                route.accept("t".into(), Event::Completed { status: TurnStatus::Error }).unwrap();
            } else { route.fail("MODEL_ERROR: original"); }
            route.aborted();
            route.fail("AgentdAbortTimeout");
            assert!(route.drain());
            let error = receiver.recv_timeout(std::time::Duration::from_secs(1)).unwrap();
            assert_eq!(error.kind, "error");
            assert_eq!(error.text.as_deref(), Some("MODEL_ERROR: original"));
            assert_eq!(receiver.recv_timeout(std::time::Duration::from_secs(1)).unwrap().kind, "done");
        }
    }
}

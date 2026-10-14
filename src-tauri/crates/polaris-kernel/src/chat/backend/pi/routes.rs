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
        let events = self.translator.accept(event);
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
        if let Some((error, _)) = &mut self.terminal {
            *error = Some(self.translator.make("error", Some(message.into())));
            return;
        }
        let events = self.translator.flush();
        self.enqueue(events);
        self.terminal = Some((Some(self.translator.make("error", Some(message.into()))), self.translator.make("done", None)));
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
    fn drop(&mut self) { self.reservation.0.lock().remove(&self.reservation.1); }
}

//! Bounded mailbox. Terminal slots are reserved, so a stalled consumer cannot
//! prevent cancellation/crash completion. Queued deltas always precede terminals.
use parking_lot::{Condvar, Mutex};
use std::{collections::VecDeque, sync::Arc, time::{Duration, Instant}};
use crate::chat::ChatStreamEvent;

#[derive(Debug)]
pub enum StreamError { Timeout, Closed }
impl std::fmt::Display for StreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self { Self::Timeout => "EventStreamTimeout", Self::Closed => "EventStreamClosed" })
    }
}
impl std::error::Error for StreamError {}

struct State {
    queue: VecDeque<ChatStreamEvent>,
    terminal: VecDeque<ChatStreamEvent>,
    finished: bool,
    receiver: bool,
}
pub struct Mailbox {
    state: Mutex<State>,
    ready: Condvar,
    capacity: usize,
    max_bytes: usize,
}
pub struct EventStream { pub(crate) mailbox: Arc<Mailbox> }
impl Mailbox {
    pub fn channel(capacity: usize, max_bytes: usize) -> (Arc<Self>, EventStream) {
        let mailbox = Arc::new(Self {
            state: Mutex::new(State { queue: VecDeque::new(), terminal: VecDeque::new(), finished: false, receiver: true }),
            ready: Condvar::new(), capacity, max_bytes,
        });
        (mailbox.clone(), EventStream { mailbox })
    }
    pub fn try_push(&self, event: ChatStreamEvent) -> Result<(), ChatStreamEvent> {
        let mut s = self.state.lock();
        if !s.receiver || s.finished { return Ok(()); }
        if event.kind == "delta" {
            if let Some(last) = s.queue.back_mut().filter(|e| e.kind == "delta") {
                if let (Some(a), Some(b)) = (&mut last.text, &event.text) {
                    if a.len() + b.len() <= self.max_bytes {
                        a.push_str(b);
                        self.ready.notify_all();
                        return Ok(());
                    }
                }
            }
        }
        if s.queue.len() >= self.capacity { return Err(event); }
        s.queue.push_back(event);
        self.ready.notify_all();
        Ok(())
    }
    pub fn finish(&self, error: Option<ChatStreamEvent>, done: ChatStreamEvent) {
        let mut s = self.state.lock();
        if s.finished { return; }
        s.finished = true;
        if let Some(error) = error { s.terminal.push_back(error); }
        s.terminal.push_back(done);
        self.ready.notify_all();
    }
    pub fn push_blocking(&self, mut event: ChatStreamEvent) {
        loop {
            match self.try_push(event) {
                Ok(()) => return,
                Err(e) => event = e,
            }
            let mut s = self.state.lock();
            if s.finished || !s.receiver { return; }
            if s.queue.len() < self.capacity { continue; }
            self.ready.wait(&mut s);
        }
    }
    pub fn is_finished(&self) -> bool { self.state.lock().finished }
}
impl EventStream {
    pub fn recv_timeout(&self, timeout: Duration) -> anyhow::Result<ChatStreamEvent> {
        let deadline = Instant::now() + timeout;
        let mut s = self.mailbox.state.lock();
        loop {
            if let Some(event) = s.queue.pop_front().or_else(|| s.terminal.pop_front()) {
                self.mailbox.ready.notify_all();
                return Ok(event);
            }
            anyhow::ensure!(!s.finished, StreamError::Closed);
            let remaining = deadline.saturating_duration_since(Instant::now());
            anyhow::ensure!(!remaining.is_zero(), StreamError::Timeout);
            self.mailbox.ready.wait_for(&mut s, remaining);
        }
    }
}
impl Drop for EventStream {
    fn drop(&mut self) {
        self.mailbox.state.lock().receiver = false;
        self.mailbox.ready.notify_all();
    }
}

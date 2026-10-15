//! Bounded admission and execution. Blocking work owns permits until it actually exits.
use crate::collab::interconnect::{self, hardening::MailDelivery};
use once_cell::sync::OnceCell;
use std::sync::{Arc, atomic::{AtomicU8, AtomicU64, Ordering}};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

static POOL: OnceCell<Pool> = OnceCell::new();
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Stage { Lookup, Hash, Smtp, Store }

impl Stage {
    pub fn name(self) -> &'static str {
        match self { Self::Lookup => "lookup", Self::Hash => "hash", Self::Smtp => "smtp", Self::Store => "store" }
    }
}

pub struct Attempt {
    id: u64,
    deadline: Instant,
    stage: AtomicU8,
}

impl Attempt {
    pub fn new(timeout: Duration) -> Self {
        Self { id: NEXT_ID.fetch_add(1, Ordering::Relaxed), deadline: Instant::now() + timeout,
            stage: AtomicU8::new(Stage::Lookup as u8) }
    }

    pub fn remaining(&self) -> Result<Duration, Stage> {
        self.deadline.checked_duration_since(Instant::now()).ok_or(self.stage())
    }

    fn stage(&self) -> Stage {
        match self.stage.load(Ordering::Relaxed) {
            1 => Stage::Hash, 2 => Stage::Smtp, 3 => Stage::Store, _ => Stage::Lookup,
        }
    }

    pub fn run<T>(&self, stage: Stage, work: impl FnOnce() -> Result<T, String>) -> Result<T, Stage> {
        self.stage.store(stage as u8, Ordering::Relaxed);
        self.remaining()?;
        let value = work().map_err(|_| stage)?;
        self.remaining()?;
        Ok(value)
    }

    fn warn(&self, event: &str, stage: Stage) {
        // Only fixed event/stage strings and a process-local correlation ID; no raw errors.
        eprintln!("[collab-mail] id={} stage={} event={event}", self.id, stage.name());
    }
}

#[derive(Clone)]
pub struct Pool {
    slots: Arc<Semaphore>,
    workers: Arc<Semaphore>,
    timeout: Duration,
}

impl Pool {
    pub fn new(config: &MailDelivery) -> Result<Self, String> {
        config.validate()?;
        Ok(Self { slots: Arc::new(Semaphore::new(config.capacity)),
            workers: Arc::new(Semaphore::new(config.concurrency)),
            timeout: Duration::from_secs(config.timeout_secs) })
    }

    pub fn configured() -> Result<&'static Self, String> {
        POOL.get_or_try_init(|| Self::new(&interconnect::load()?.mail_delivery))
    }

    /// None means full: callers must return the same anonymous response as admission.
    pub fn submit(&self, job: impl FnOnce(&Attempt) -> Result<(), Stage> + Send + 'static)
        -> Option<tokio::task::JoinHandle<()>>
    {
        let attempt = Arc::new(Attempt::new(self.timeout));
        let Ok(slot) = self.slots.clone().try_acquire_owned() else {
            attempt.warn("queue_full", Stage::Lookup);
            return None;
        };
        let workers = self.workers.clone();
        Some(tokio::spawn(async move {
            let deadline = tokio::time::Instant::from_std(attempt.deadline);
            let worker = match tokio::time::timeout_at(deadline, workers.acquire_owned()).await {
                Ok(Ok(worker)) => worker,
                _ => { attempt.warn("timeout", Stage::Lookup); return; }
            };
            let active = attempt.clone();
            let mut task = tokio::task::spawn_blocking(move || {
                // Cancellation of a JoinHandle cannot stop blocking SMTP. Never free these early.
                let (_slot, _worker) = (slot, worker);
                active.remaining()?;
                job(&active)
            });
            match tokio::time::timeout_at(deadline, &mut task).await {
                Ok(Ok(Ok(()))) => {}
                Ok(Ok(Err(stage))) => attempt.warn("failed", stage),
                Ok(Err(_)) => attempt.warn("worker_failed", attempt.stage()),
                Err(_) => {
                    attempt.warn("timeout", attempt.stage());
                    // Abort only prevents a not-yet-started blocking job; running work retains permits.
                    task.abort();
                    let _ = task.await;
                }
            }
        }))
    }
}

#[cfg(test)]
#[path = "delivery_tests.rs"]
mod tests;

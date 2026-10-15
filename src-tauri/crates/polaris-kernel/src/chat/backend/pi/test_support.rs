//! Test-only synchronization. No process-wide clock or production configuration changes.
use super::{PiBackend, client::{Control, Reply}};
use crate::chat::backend::{AgentBackend, BackendStatus};
use std::time::{Duration, Instant};

pub(super) fn wait_budget() -> Duration {
    // Parallel Node startup/SDK loading can exceed 5s. Thirty seconds is only
    // a failure ceiling; successful state transitions return immediately.
    let value = std::env::var("POLARIS_TEST_WAIT_MS").unwrap_or_else(|_| "30000".into());
    let ms: u64 = value.parse().expect("POLARIS_TEST_WAIT_MS must be an integer");
    assert!((1000..=300_000).contains(&ms), "POLARIS_TEST_WAIT_MS must be in [1000, 300000]");
    Duration::from_millis(ms)
}

pub(super) fn wait_until(label: &str, predicate: impl Fn() -> bool) -> anyhow::Result<()> {
    let deadline = Instant::now() + wait_budget();
    while !predicate() {
        anyhow::ensure!(Instant::now() < deadline, "state deadline: {label}");
        // Poll a level-triggered state, never assume that sleeping means ready.
        std::thread::sleep(Duration::from_millis(5));
    }
    Ok(())
}

pub(super) fn wait_status(backend: &PiBackend, predicate: impl Fn(&BackendStatus) -> bool) -> anyhow::Result<BackendStatus> {
    let deadline = Instant::now() + wait_budget();
    loop {
        let status = backend.status();
        if predicate(&status) { return Ok(status); }
        anyhow::ensure!(Instant::now() < deadline, "status deadline: {status:?}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn control(backend: &PiBackend, command: impl FnOnce(Reply) -> Control) -> anyhow::Result<()> {
    let client = backend.client()?;
    let (reply, response) = std::sync::mpsc::sync_channel(1);
    client.send(command(reply))?;
    response.recv_timeout(wait_budget())?.map_err(anyhow::Error::msg)?;
    Ok(())
}

pub(super) fn age_idle(backend: &PiBackend) -> anyhow::Result<()> {
    control(backend, Control::TestAgeIdle)
}

pub(super) fn wait_pings(backend: &PiBackend, count: u32) -> anyhow::Result<()> {
    control(backend, |reply| Control::TestWaitPings(count, reply))
}

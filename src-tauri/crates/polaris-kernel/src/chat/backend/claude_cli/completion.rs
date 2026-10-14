//! Ownership follows the stdout worker or queued job, beyond preparation.
use super::*;

pub(super) const WORKER_EXITED: &str = "ClaudeCliWorkerExited";
static LAST_ERROR: Mutex<Option<String>> = Mutex::new(None);

pub(super) fn record_failure(error: &str) {
    *LAST_ERROR.lock() = Some(error.into());
}

pub(super) fn status() -> BackendStatus {
    let mut status = BackendStatus::new("claude-cli");
    status.last_error = LAST_ERROR.lock().clone();
    status.healthy = status.last_error.is_none() && crate::doctor::resolve_claude_exe().is_some();
    status
}

pub(super) struct TurnGuard {
    req: String,
    conv: Option<String>,
    stream: Option<Arc<stream::Mailbox>>,
}

impl TurnGuard {
    pub(super) fn new(req: &str, conv: Option<&str>) -> Self {
        let stream = STREAMS.lock().get(req).map(|capture| capture.stream.clone());
        Self { req: req.into(), conv: conv.map(str::to_owned), stream }
    }

    // Failed dispatch keeps ownership with preparation, which may retry.
    pub(super) fn disarm(&mut self) { self.stream = None; }
}

impl Drop for TurnGuard {
    fn drop(&mut self) {
        let Some(stream) = self.stream.take() else { return; };
        if stream.is_finished() { return; }
        record_failure(WORKER_EXITED);
        {
            let mut streams = STREAMS.lock();
            if streams.get(&self.req).is_some_and(|entry| Arc::ptr_eq(&entry.stream, &stream)) {
                streams.remove(&self.req);
            }
        }
        CHILDREN.take_cancel(&self.req);
        stream.finish(
            Some(event(&self.req, self.conv.as_deref(), "error", Some(WORKER_EXITED.into()))),
            event(&self.req, self.conv.as_deref(), "done", None),
        );
    }
}

#[cfg(test)]
pub(super) fn fixture(req: &str) -> (TurnGuard, EventStream) {
    let (stream, receiver) = stream::Mailbox::channel(1, 4096);
    STREAMS.lock().insert(req.into(), Capture { stream, session: req.into() });
    (TurnGuard::new(req, Some(req)), receiver)
}

#[cfg(test)]
pub(super) fn assert_failed(req: &str, receiver: EventStream) -> anyhow::Result<()> {
    let wait = Duration::from_secs(1);
    let error = receiver.recv_timeout(wait)?;
    assert_eq!(error.req_id, req);
    assert_eq!(error.conversation_id.as_deref(), Some(req));
    assert_eq!(error.kind, "error");
    assert_eq!(error.text.as_deref(), Some(WORKER_EXITED));
    assert_eq!(receiver.recv_timeout(wait)?.kind, "done");
    let closed = receiver.recv_timeout(wait).err().ok_or_else(|| anyhow::anyhow!("duplicate terminal"))?;
    assert!(matches!(closed.downcast_ref(), Some(stream::StreamError::Closed)));
    assert!(!STREAMS.lock().contains_key(req));
    let status = ClaudeCliBackend.status();
    assert!(!status.healthy);
    assert_eq!(status.last_error.as_deref(), Some(WORKER_EXITED));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stdout_worker_return_and_panic_finish_full_mailbox() -> anyhow::Result<()> {
        for panic in [false, true] {
            let req = format!("stdout-worker-{panic}");
            let (guard, receiver) = fixture(&req);
            assert!(capture(&event(&req, Some(&req), "meta", None)));
            let worker = std::thread::spawn(move || {
                let _completion = guard;
                assert!(!panic, "injected stdout worker panic");
            });
            assert_eq!(worker.join().is_err(), panic);
            assert_eq!(receiver.recv_timeout(Duration::from_secs(1))?.kind, "meta");
            assert_failed(&req, receiver)?;
        }
        Ok(())
    }

    #[test]
    fn finished_worker_does_not_duplicate_done() -> anyhow::Result<()> {
        let req = "finished-worker";
        let (guard, receiver) = fixture(req);
        assert!(capture(&event(req, Some(req), "done", None)));
        drop(guard);
        assert_eq!(receiver.recv_timeout(Duration::from_secs(1))?.kind, "done");
        let error = receiver.recv_timeout(Duration::from_secs(1)).err()
            .ok_or_else(|| anyhow::anyhow!("duplicate terminal"))?;
        assert!(matches!(error.downcast_ref(), Some(stream::StreamError::Closed)));
        Ok(())
    }
}

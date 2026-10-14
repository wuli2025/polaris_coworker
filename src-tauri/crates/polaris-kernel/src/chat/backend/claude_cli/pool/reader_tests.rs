use super::*;
use super::super::super::completion;

fn job(req: &str) -> (TurnJob, crate::chat::backend::EventStream) {
    let (completion, receiver) = completion::fixture(req);
    let (tx, _) = tokio::sync::broadcast::channel(8);
    (TurnJob {
        completion, req_id: req.into(), conv_id: req.into(),
        app: AppHandle::new(tx), art_dir: std::env::temp_dir(),
        art_before: HashMap::new(), image_notice: None,
        watchdog: Arc::new(polaris_watchdog::Watchdog::new(
            polaris_watchdog::WatchdogConfig { idle_secs: 0, hard_cap_secs: 0 })),
    }, receiver)
}

#[test]
fn persistent_reader_panic_finishes_active_and_queued_jobs() -> anyhow::Result<()> {
    let (active, active_stream) = job("reader-active");
    let (queued, queued_stream) = job("reader-queued");
    let shared = Arc::new(SessionShared {
        conv: "reader-active".into(), pid: 0, alive: AtomicBool::new(true),
        kill_reported: AtomicBool::new(false), active_req: Mutex::new(Some("reader-active".into())),
        watchdog: Mutex::new(Some(active.watchdog.clone())), stderr_tail: Mutex::new(String::new()),
    });
    let (tx, rx) = mpsc::channel();
    assert!(tx.send(active).is_ok());
    assert!(tx.send(queued).is_ok());
    let state = shared.clone();
    let worker = std::thread::spawn(move || {
        read_jobs(std::iter::from_fn(|| panic!("injected persistent reader panic")), rx, state);
    });
    completion::assert_failed("reader-active", active_stream)?;
    completion::assert_failed("reader-queued", queued_stream)?;
    assert!(worker.join().is_err());
    assert!(!shared.alive.load(Ordering::SeqCst));
    assert!(shared.active_req.lock().is_none());
    assert!(shared.watchdog.lock().is_none());
    Ok(())
}

#[test]
fn dropped_job_receiver_finishes_queued_turn() -> anyhow::Result<()> {
    let (job, stream) = job("reader-channel-closed");
    let (tx, rx) = mpsc::channel();
    assert!(tx.send(job).is_ok());
    drop(rx);
    completion::assert_failed("reader-channel-closed", stream)
}

#[test]
fn rejected_job_keeps_mailbox_open_for_preparation_retry() -> anyhow::Result<()> {
    let (job, stream) = job("reader-retry");
    let (tx, rx) = mpsc::channel();
    drop(rx);
    let mut rejected = tx.send(job).err().ok_or_else(|| anyhow::anyhow!("dispatch succeeded"))?;
    rejected.0.completion.disarm();
    drop(rejected);
    assert!(!stream.mailbox.is_finished());
    assert!(super::super::super::capture(&crate::chat::backend::event("reader-retry", None, "done", None)));
    assert_eq!(stream.recv_timeout(std::time::Duration::from_secs(1))?.kind, "done");
    Ok(())
}

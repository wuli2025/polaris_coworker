use super::*;

#[tokio::test]
async fn full_queue_and_timeouts_preserve_concurrency_and_release_slots() -> Result<(), String> {
    let mut pool = Pool::new(&MailDelivery { capacity: 2, concurrency: 1, timeout_secs: 1 })?;
    pool.timeout = Duration::from_millis(100);
    let (started, ready) = tokio::sync::oneshot::channel();
    let (release, wait) = std::sync::mpsc::channel();
    let first = pool.submit(move |_| {
        let _ = started.send(());
        wait.recv_timeout(Duration::from_secs(5)).map_err(|_| Stage::Smtp)?;
        Ok(())
    }).ok_or("first admission failed")?;
    ready.await.map_err(|e| e.to_string())?;
    let second = pool.submit(|_| panic!("expired queued job must never run")).ok_or("queue admission failed")?;
    assert!(pool.submit(|_| Ok(())).is_none());
    second.await.map_err(|e| e.to_string())?;
    assert_eq!(pool.slots.available_permits(), 1, "queued timeout frees its capacity");
    assert_eq!(pool.workers.available_permits(), 0, "timed-out running work still owns its permit");
    release.send(()).map_err(|e| e.to_string())?;
    first.await.map_err(|e| e.to_string())?;
    assert_eq!(pool.slots.available_permits(), 2);
    assert_eq!(pool.workers.available_permits(), 1);
    pool.submit(|_| Ok(())).ok_or("capacity was not released")?.await.map_err(|e| e.to_string())?;
    Ok(())
}

#[test]
fn failure_stages_are_fixed_and_do_not_include_sender_errors() {
    for stage in [Stage::Lookup, Stage::Hash, Stage::Smtp, Stage::Store] {
        let attempt = Attempt::new(Duration::from_secs(1));
        let result: Result<(), _> = attempt.run(stage, || Err("private@example.com 123456".into()));
        assert_eq!(result, Err(stage));
        assert!(!format!("{result:?}").contains("private"));
        assert_eq!(attempt.stage(), stage);
    }
}

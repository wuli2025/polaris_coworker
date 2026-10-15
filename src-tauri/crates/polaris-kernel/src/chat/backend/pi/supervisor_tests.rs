use super::*;

#[test]
fn drop_replies_to_every_deferred_and_pending_request_immediately() -> anyhow::Result<()> {
    for panic in [false, true] {
        let now = Instant::now();
        let status = Arc::new(Mutex::new(BackendStatus::new("pi")));
        let mut state = State {
            experts: Default::default(),
            config: super::super::tests::config(), status: status.clone(),
            active: Arc::new(Mutex::new(HashMap::new())), routes: HashMap::new(),
            pending: HashMap::new(), deferred: Vec::new(), counter: 0, failures: 0,
            next_start: now, last_ping: now, started: now, last_used: now, stopping: false, ping_waiters: Vec::new(),
        };
        let mut receivers = Vec::new();
        // Keep sender clones alive: channel disconnection cannot pass this test.
        let mut senders = Vec::new();
        for index in 0..3 {
            let (reply, rx) = std::sync::mpsc::sync_channel(1);
            senders.push(reply.clone());
            receivers.push(rx);
            state.deferred.push((Command::Ping(Empty {}), reply, now));
            let (reply, rx) = std::sync::mpsc::sync_channel(1);
            senders.push(reply.clone());
            receivers.push(rx);
            state.pending.insert(Id::Text(format!("pending-{index}")),
                Pending { reply: Some(reply), since: now, ping: false });
        }
        state.pending.insert(Id::Text("health-ping".into()),
            Pending { reply: None, since: now, ping: true });
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let _state = state;
            assert!(!panic, "injected supervisor unwind");
        }));
        assert_eq!(outcome.is_err(), panic);
        for rx in receivers {
            assert_eq!(rx.try_recv()?, Err(super::super::routes::SUPERVISOR_EXITED.into()));
        }
        assert_eq!(status.lock().last_error.as_deref(), Some(super::super::routes::SUPERVISOR_EXITED));
        drop(senders);
    }
    Ok(())
}

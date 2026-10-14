use super::*;
use super::protocol::{Event, Frame, TurnStatus};
use std::time::Instant;
fn config() -> config::Agentd {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let mut c = config::Agentd::default();
    c.entry = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-agentd.mjs");
    c.isolation = "shared".into();
    c.request_timeout_ms = 1500;
    c.turn_timeout_ms = 3000;
    c.abort_timeout_ms = 100;
    c.restart_base_ms = 20;
    c.restart_max_ms = 80;
    c.restart_jitter_ms = 10;
    c.stable_reset_ms = 10000;
    c.failure_threshold = 3;
    c.max_line_bytes = 4096;
    c.channel_capacity = 2;
    c.session_map = std::env::temp_dir().join(format!("pi-tests-{}-{}", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    c
}
fn input(req: &str, prompt: &str) -> TurnRequest {
    TurnRequest { req_id: req.into(), session_id: req.into(), conversation_id: Some(req.into()),
        prompt: prompt.into(), permission: PermissionMode::Deny, model: None,
        cwd: std::env::temp_dir(), attachments: vec![], account_id: None, timeout: None, legacy: None }
}
fn collect_events(stream: EventStream) -> Vec<crate::chat::ChatStreamEvent> {
    let mut events = vec![];
    loop {
        let e = stream.recv_timeout(Duration::from_secs(5)).expect("terminal event deadline");
        let done = e.kind == "done";
        events.push(e);
        if done { return events; }
    }
}
fn ready(b: &PiBackend) {
    b.models().unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while !b.status().healthy {
        assert!(Instant::now() < deadline, "{:?}", b.status());
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn protocol_samples_and_source_fingerprint() {
    use sha2::{Digest, Sha256};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let sources = [root.join("../../../agentd/src/protocol/schema.ts"), root.join("../../../agentd/src/protocol/events.ts")];
    let hashes = include_str!("../../../..//tests/fixtures/protocol-schema.sha256");
    for (path, hash) in sources.iter().zip(hashes.lines()) {
        assert_eq!(format!("{:x}", Sha256::digest(std::fs::read(path).unwrap())), hash,
            "agentd protocol changed: review Rust types AND sample fixture");
    }
    for line in include_str!("../../../../tests/fixtures/protocol-samples.jsonl").lines() {
        let v: Value = serde_json::from_str(line).unwrap();
        if v.get("method").is_some() {
            let request: protocol::Request = serde_json::from_str(line).unwrap();
            assert_eq!(serde_json::to_value(request).unwrap(), v);
        } else { Frame::parse(line.as_bytes()).unwrap(); }
    }
    assert!(Frame::parse(br#"{"type":"event","event":"turn.completed","sessionId":"s","turnId":"t","status":"success"}"#).is_err());
    assert!(Frame::parse(br#"{"type":"response","id":1,"ok":false}"#).is_err());
    assert!(Frame::parse(br#"{"type":"response","id":1,"ok":true,"result":null}"#).is_ok());
}
#[test]
fn translation_covers_every_event_and_order() {
    let c = config();
    let mut t = translate::Translator::new("r".into(), Some("c".into()), PathBuf::from("/tmp"), &c);
    assert!(t.accept(Event::Started).is_empty());
    assert!(t.accept(Event::Text { text: "hello".into() }).is_empty());
    let tool = t.accept(Event::ToolStarted { call_id: "call".into(), tool: "write".into(), args: serde_json::json!({"path":"report.md"}), parent_call_id: None });
    assert_eq!(tool.iter().map(|e| e.kind.as_str()).collect::<Vec<_>>(), ["delta", "tool"]);
    let artifact = t.accept(Event::ToolCompleted { call_id: "call".into(), tool: "write".into(), result: Value::Null, is_error: false, parent_call_id: None });
    assert_eq!(artifact[0].kind, "artifact");
    assert_eq!(artifact[0].text.as_deref(), Some("/tmp/report.md"));
    assert!(t.accept(Event::Reasoning { text: "hidden".into() }).is_empty());
    let failed = t.accept(Event::ToolCompleted { call_id: "bad".into(), tool: "edit".into(), result: Value::Null, is_error: true, parent_call_id: None });
    assert_eq!(failed[0].kind, "error");
    let failed = t.accept(Event::Completed { status: TurnStatus::Error });
    assert_eq!(failed.iter().map(|e| e.kind.as_str()).collect::<Vec<_>>(), ["error", "done"]);
    assert!(t.accept(Event::Completed { status: TurnStatus::Completed }).is_empty());
    let mut t = translate::Translator::new("r".into(), None, PathBuf::from("/tmp"), &c);
    assert_eq!(t.accept(Event::Error { code: "E".into(), message: "bad".into() })[0].kind, "error");
    assert_eq!(t.accept(Event::Completed { status: TurnStatus::Error }).len(), 1);
}
#[test]
fn daemon_stream_routing_and_collect() {
    let b = PiBackend::new(config()).unwrap();
    ready(&b);
    let a = b.start(input("a", "normal")).unwrap();
    let c = b.start(input("c", "failure")).unwrap();
    let a = collect_events(a);
    let c = collect_events(c);
    assert_eq!(a[0].kind, "meta");
    assert_eq!(a.iter().filter(|e| e.kind == "delta").filter_map(|e| e.text.as_deref()).collect::<String>(), "hello world");
    assert!(a.iter().all(|e| e.conversation_id.as_deref() == Some("a")));
    assert_eq!(c[c.len()-2].kind, "error");
    assert_eq!(collect(&b, input("headless", "normal"), |_,_| {}).unwrap(), "hello world");
}
#[test]
fn crash_restarts_and_active_turn_finishes() {
    let b = PiBackend::new(config()).unwrap(); ready(&b);
    let events = collect_events(b.start(input("crash", "crash")).unwrap());
    assert_eq!(events[events.len()-2].kind, "error");
    let deadline = Instant::now() + Duration::from_secs(3);
    while b.status().restarts == 0 || !b.status().healthy {
        assert!(Instant::now() < deadline); std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(collect(&b, input("recovered", "normal"), |_,_| {}).unwrap(), "hello world");
}
#[test]
fn cancellation_and_abort_timeout_both_finish() {
    for prompt in ["hang", "abort-hang"] {
        let b = PiBackend::new(config()).unwrap(); ready(&b);
        let stream = b.start(input(prompt, prompt)).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while b.client.get().unwrap().active.lock().is_empty() {
            assert!(Instant::now() < deadline); std::thread::sleep(Duration::from_millis(5));
        }
        b.cancel(prompt).unwrap();
        assert_eq!(collect_events(stream).last().unwrap().kind, "done");
    }
}
#[test]
fn malformed_and_oversized_lines_fail_closed() {
    for prompt in ["malformed", "oversized"] {
        let b = PiBackend::new(config()).unwrap(); ready(&b);
        let events = collect_events(b.start(input(prompt, prompt)).unwrap());
        assert_eq!(events[events.len()-2].kind, "error");
    }
}
#[test]
fn slow_consumer_retains_text_and_terminal() {
    let b = PiBackend::new(config()).unwrap(); ready(&b);
    let stream = b.start(input("slow", "flood")).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    let events = collect_events(stream);
    assert!(!events.iter().any(|e| e.kind == "error"), "{events:?}");
    assert_eq!(events.iter().filter(|e| e.kind == "delta").filter_map(|e| e.text.as_ref()).map(String::len).sum::<usize>(), 128000);
    assert_eq!(events.iter().filter(|e| e.kind == "tool").count(), 1000);
    assert_eq!(events.iter().filter(|e| e.kind == "done").count(), 1);
}
#[test]
fn repeated_startup_failure_opens_circuit() {
    let mut c = config(); c.args = vec!["--crash-start".into()];
    let b = PiBackend::new(c).unwrap(); let _ = b.models();
    let deadline = Instant::now() + Duration::from_secs(4);
    while !b.status().circuit_open { assert!(Instant::now() < deadline); std::thread::sleep(Duration::from_millis(10)); }
    assert!(b.start(input("blocked", "normal")).err().unwrap().to_string().contains("CircuitOpen"));
}
#[test]
fn isolation_never_silently_shares() {
    let mut c = config(); c.isolation = "per-account".into();
    let b = PiBackend::new(c).unwrap();
    assert!(b.start(input("u", "normal")).err().unwrap().to_string().contains("AccountContextRequired"));
    assert!(b.status().pid.is_none());
}

#[test]
fn turn_and_ping_timeouts_are_bounded() {
    let mut c = config(); c.turn_timeout_ms = 80;
    let b = PiBackend::new(c).unwrap(); ready(&b);
    let events = collect_events(b.start(input("timeout", "hang")).unwrap());
    assert!(events.iter().any(|e| e.text.as_deref().is_some_and(|s| s.contains("Timeout"))));
    let mut c = config(); c.args = vec!["--ping-hang".into()]; c.request_timeout_ms = 80;
    let b = PiBackend::new(c).unwrap(); let _ = b.models();
    let deadline = Instant::now() + Duration::from_secs(3);
    while !b.status().circuit_open { assert!(Instant::now() < deadline); std::thread::sleep(Duration::from_millis(10)); }
}
#[test]
fn idle_reaping_and_reopen_after_config_apply() {
    let mut c = config(); c.idle_ms = 100;
    let b = PiBackend::new(c).unwrap(); ready(&b);
    assert_eq!(collect(&b, input("session", "normal"), |_,_| {}).unwrap(), "hello world");
    b.apply_config(Some(serde_json::json!({"thinkingLevel":"off"})), None).unwrap();
    assert_eq!(collect(&b, input("session", "normal"), |_,_| {}).unwrap(), "hello world");
    let deadline = Instant::now() + Duration::from_secs(3);
    while b.status().pid.is_some() { assert!(Instant::now() < deadline); std::thread::sleep(Duration::from_millis(10)); }
    assert_eq!(collect(&b, input("after-idle", "normal"), |_,_| {}).unwrap(), "hello world");
}
#[test]
fn mailbox_reserves_terminals_and_preserves_order() {
    let (mailbox, stream) = stream::Mailbox::channel(1, 10);
    mailbox.try_push(event("r", None, "tool", None)).unwrap();
    assert!(mailbox.try_push(event("r", None, "delta", Some("pending".into()))).is_err());
    mailbox.finish(Some(event("r", None, "error", Some("crash".into()))), event("r", None, "done", None));
    let kinds = collect_events(stream).into_iter().map(|e| e.kind).collect::<Vec<_>>();
    assert_eq!(kinds, ["tool", "error", "done"]);
}

#[test]
fn usage_and_all_completion_statuses_have_expected_sequences() {
    let usage = serde_json::json!({"input":1,"output":2,"cacheRead":0,"cacheWrite":0,"totalTokens":3,
        "cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}});
    let mut t = translate::Translator::new("r".into(), None, PathBuf::from("/tmp"), &config());
    assert!(t.accept(Event::Usage { usage: serde_json::from_value(usage).unwrap() }).is_empty());
    for status in [TurnStatus::Completed, TurnStatus::Aborted, TurnStatus::Error, TurnStatus::Timeout] {
        let mut t = translate::Translator::new("r".into(), None, PathBuf::from("/tmp"), &config());
        let expected = if matches!(status, TurnStatus::Error | TurnStatus::Timeout) { vec!["error", "done"] } else { vec!["done"] };
        let events = t.accept(Event::Completed { status });
        assert_eq!(events.iter().map(|e| e.kind.as_str()).collect::<Vec<_>>(), expected);
    }
}

#[test]
fn config_apply_is_not_busy_after_circuit_or_thread_exit() {
    for shutdown in [false, true] {
        let mut c = config(); c.failure_threshold = 1;
        let b = PiBackend::new(c).unwrap(); ready(&b);
        let stream = b.start(input("release", if shutdown { "hang" } else { "crash" })).unwrap();
        if shutdown {
            let deadline = Instant::now() + Duration::from_secs(2);
            while b.client.get().unwrap().active.lock().is_empty() {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(5));
            }
            b.shutdown();
        }
        let events = collect_events(stream);
        assert_eq!(events.last().unwrap().kind, "done");
        let deadline = Instant::now() + Duration::from_secs(2);
        while !b.client.get().unwrap().active.lock().is_empty() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        let error = b.apply_config(None, None).unwrap_err().to_string();
        assert!(!error.contains("AgentdConfigBusy"), "{error}");
        assert!(error.contains(if shutdown { "AgentdSupervisorStopped" } else { "AgentdCircuitOpen" }), "{error}");
    }
}

#[test]
fn stalled_stdin_reports_backpressure_to_caller_and_status() {
    let mut c = config();
    c.node_args = vec!["-e".into(), "setInterval(() => {}, 1000)".into()];
    c.request_timeout_ms = 300;
    c.max_line_bytes = 2_000_000;
    c.failure_threshold = 1;
    let b = PiBackend::new(c).unwrap();
    let client = b.client().unwrap();
    let error = client.call(Command::Prompt(Text { session_id: "s".into(), text: "x".repeat(1_000_000) })).unwrap_err().to_string();
    assert!(error.contains("AgentdWriterBackpressure"), "{error}");
    assert!(!error.contains("RequestTimeout"), "{error}");
    let status = b.status();
    assert!(status.circuit_open);
    assert!(status.last_error.unwrap().contains("AgentdWriterBackpressure"));
}

#[test]
fn supervisor_exit_clears_orphan_active_entries() {
    let b = PiBackend::new(config()).unwrap(); ready(&b);
    let client = b.client.get().unwrap();
    // Simulate an orphan not owned by a Route; only supervisor exit can clear it.
    client.active.lock().insert("orphan".into(), "session".into());
    assert!(b.apply_config(None, None).unwrap_err().to_string().contains("AgentdConfigBusy"));
    b.shutdown();
    let deadline = Instant::now() + Duration::from_secs(2);
    while !client.active.lock().is_empty() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!b.apply_config(None, None).unwrap_err().to_string().contains("AgentdConfigBusy"));
}

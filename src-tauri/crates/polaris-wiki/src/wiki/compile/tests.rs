use super::*;
use polaris_kernel::chat::{ChatStreamEvent, PermissionMode, backend::{
    AgentBackend, BackendStatus, EventStream, TurnRequest, ToolOptions, stream::Mailbox,
    pi::protocol::{Decision, PermissionResponse},
}};
use serde_json::json;
use std::{path::PathBuf, sync::atomic::AtomicUsize};

struct Temp(PathBuf);
impl Temp {
    fn new() -> anyhow::Result<Self> {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!("wiki-p5b-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::SeqCst)));
        std::fs::create_dir_all(root.join("wiki"))?;
        std::fs::create_dir_all(root.join("raw"))?;
        std::fs::write(root.join("raw/source.md"), "immutable source")?;
        Ok(Self(std::fs::canonicalize(root)?))
    }
}
impl Drop for Temp { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }
fn event(kind: &str, text: Option<String>) -> ChatStreamEvent {
    ChatStreamEvent { req_id: "test".into(), conversation_id: None, kind: kind.into(), text,
        tool: None, call_id: None, parent_call_id: None }
}
fn tool(kind: &str, id: &str, path: &str, failed: bool) -> ChatStreamEvent {
    event(kind, Some(json!({"callId":id,"tool":"write","args":{"path":path},"isError":failed}).to_string()))
}
struct Fake {
    kind: String,
    events: Vec<ChatStreamEvent>,
    terminal: Option<String>,
    fail_start: bool,
    starts: AtomicUsize,
    aborts: AtomicUsize,
    closes: AtomicUsize,
    denials: AtomicUsize,
    root: PathBuf,
    stream: parking_lot::Mutex<Option<Arc<Mailbox>>>,
}
impl Fake {
    fn new(root: &Temp, events: Vec<ChatStreamEvent>, terminal: Option<&str>) -> Arc<Self> {
        Arc::new(Self { kind: "pi".into(), events, terminal: terminal.map(str::to_owned), fail_start: false,
            starts: AtomicUsize::new(0), aborts: AtomicUsize::new(0), closes: AtomicUsize::new(0),
            denials: AtomicUsize::new(0), root: root.0.join("wiki"), stream: parking_lot::Mutex::new(None) })
    }
}
impl AgentBackend for Fake {
    fn start(&self, _: TurnRequest) -> anyhow::Result<EventStream> { anyhow::bail!("tool options required") }
    fn start_with_tools(&self, input: TurnRequest, options: ToolOptions) -> anyhow::Result<EventStream> {
        assert_eq!(input.cwd, self.root);
        assert!(matches!(input.permission, PermissionMode::AutoCurrent));
        assert!(options.sandbox);
        assert_eq!(options.tool_profile.as_deref(), Some("wiki-custom"));
        assert!(options.detailed_events);
        assert_eq!(input.timeout, Some(std::time::Duration::from_secs(1)));
        assert!(input.prompt.contains("AGENTS.md"));
        self.starts.fetch_add(1, Ordering::SeqCst);
        anyhow::ensure!(!self.fail_start, "FakeStartFailed");
        let (mailbox, stream) = Mailbox::channel(64, 65536);
        for event in &self.events { mailbox.push_blocking(event.clone()); }
        if let Some(status) = &self.terminal {
            mailbox.finish(None, event("done", Some(status.clone())));
        }
        *self.stream.lock() = Some(mailbox);
        Ok(stream)
    }
    fn permission_respond(&self, response: PermissionResponse) -> anyhow::Result<()> {
        assert!(matches!(response.decision, Decision::Deny));
        self.denials.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn cancel(&self, _: &str) -> anyhow::Result<()> {
        self.aborts.fetch_add(1, Ordering::SeqCst);
        if let Some(mailbox) = self.stream.lock().as_ref() {
            mailbox.finish(None, event("done", Some("Aborted".into())));
        }
        Ok(())
    }
    fn close(&self, _: &str) -> anyhow::Result<()> { self.closes.fetch_add(1, Ordering::SeqCst); Ok(()) }
    fn models(&self) -> anyhow::Result<serde_json::Value> { Ok(json!([])) }
    fn status(&self) -> BackendStatus {
        BackendStatus {
            backend: self.kind.clone(), agentd_version: None, pid: None, restarts: 0,
            usage_persist_failures: 0, experts_synced: (self.kind == "pi").then_some(false),
            expert_sync_error: None, circuit_open: false, healthy: true, last_error: None,
        }
    }
    fn is_running(&self, _: &str) -> bool { false }
    fn shutdown(&self) {}
}
fn config() -> config::CompileConfig {
    config::CompileConfig { tool_profile: "wiki-custom".into(), sandbox: true, timeout_secs: 1, abort_grace_ms: 100 }
}
fn run(fake: Arc<Fake>, root: &Temp) -> Vec<KbCompileEvent> {
    let mut events = Vec::new();
    let mut reindexed = 0;
    runner::run(fake, "test", &root.0, &config(), |event| events.push(event), || { reindexed += 1; 17 });
    assert_eq!(reindexed, 1);
    assert_eq!(events.iter().filter(|e| e.kind == "done").count(), 1);
    assert_eq!(events.last().and_then(|e| e.doc_count), Some(17));
    events
}
#[test]
fn successful_write_edit_and_text_preserve_progress_contract() -> anyhow::Result<()> {
    let root = Temp::new()?;
    let mut edit = tool("tool.started", "two", "concepts/one.md", false);
    edit.text = Some(json!({"callId":"two","tool":"edit","args":{"path":"concepts/one.md"}}).to_string());
    let fake = Fake::new(&root, vec![
        event("delta", Some("正在编译".into())),
        tool("tool.started", "one", "concepts/one.md", false),
        tool("tool.completed", "one", "concepts/one.md", false),
        edit, tool("tool.completed", "two", "concepts/one.md", false),
        tool("tool.started", "failed", "missing.md", false),
        tool("tool.completed", "failed", "missing.md", true),
    ], Some("Completed"));
    let events = run(fake.clone(), &root);
    assert_eq!(events.iter().filter(|e| e.kind == "page").count(), 2);
    assert!(events.iter().any(|e| e.text.as_deref() == Some("写入 concepts/one.md")));
    assert!(events.iter().any(|e| e.kind == "delta" && e.text.as_deref() == Some("正在编译")));
    assert!(events.last().and_then(|e| e.text.as_ref()).is_some_and(|s| s.contains("1 个页面")));
    assert_eq!(fake.closes.load(Ordering::SeqCst), 1);
    assert_eq!(fake.aborts.load(Ordering::SeqCst), 0);
    Ok(())
}
#[test]
fn out_of_directory_writes_are_rejected_and_never_reported_as_pages() -> anyhow::Result<()> {
    let root = Temp::new()?;
    for path in ["../raw/source.md".to_owned(), root.0.join("raw/source.md").to_string_lossy().into()] {
        let fake = Fake::new(&root, vec![tool("tool.started", "bad", &path, false)], Some("Completed"));
        let events = run(fake.clone(), &root);
        assert!(!events.iter().any(|e| e.kind == "page"));
        assert!(events.iter().any(|e| e.kind == "error" && e.text.as_ref().is_some_and(|s| s.contains("WikiWriteOutsideRoot"))));
        assert!(fake.aborts.load(Ordering::SeqCst) > 0);
    }
    // Actual Pi authorization happens before execution. Headless never approves
    // out-of-root writes or shell calls, irrespective of the configured profile.
    let fake = Fake::new(&root, vec![event("permission.request", Some(json!({
        "sessionId":"wiki", "callId":"outside", "tool":"write", "summary":"../raw/source.md",
    }).to_string()))], Some("Completed"));
    run(fake.clone(), &root);
    assert_eq!(fake.denials.load(Ordering::SeqCst), 1);
    assert_eq!(std::fs::read_to_string(root.0.join("raw/source.md"))?, "immutable source");
    Ok(())
}
#[test]
fn timeout_and_cancel_abort_close_reindex_and_release_maintenance_lock() -> anyhow::Result<()> {
    let root = Temp::new()?;
    for cancel in [false, true] {
        let guard = acquire_kb_task().map_err(anyhow::Error::msg)?;
        assert!(acquire_kb_task().is_err());
        let fake = Fake::new(&root, vec![], None);
        let cancelling = if cancel {
            let fake = fake.clone();
            Some(std::thread::spawn(move || {
                while fake.stream.lock().is_none() { std::thread::yield_now(); }
                fake.cancel("test")
            }))
        } else { None };
        let started = std::time::Instant::now();
        let events = run(fake.clone(), &root);
        assert!(started.elapsed() < std::time::Duration::from_secs(3));
        assert!(events.iter().any(|e| e.kind == "error"));
        assert!(events.last().and_then(|e| e.text.as_ref()).is_some_and(|s| s.contains("中断")));
        assert!(fake.aborts.load(Ordering::SeqCst) > 0);
        assert_eq!(fake.closes.load(Ordering::SeqCst), 1);
        if let Some(thread) = cancelling { thread.join().map_err(|_| anyhow::anyhow!("cancel thread panicked"))??; }
        drop(guard);
        drop(acquire_kb_task().map_err(anyhow::Error::msg)?);
    }
    Ok(())
}
#[test]
fn backend_guard_and_configuration_fail_closed() -> anyhow::Result<()> {
    let root = Temp::new()?;
    let fake = Fake::new(&root, vec![], None);
    assert!(!legacy_selected(fake.as_ref(), &config())?);
    let mut fake = Arc::try_unwrap(fake).map_err(|_| anyhow::anyhow!("fake still shared"))?;
    fake.kind = "claude-cli".into();
    assert!(legacy_selected(&fake, &config()).is_err());
    assert!(legacy_selected(&fake, &config::CompileConfig::default())?);
    fake.kind = "typo".into();
    assert!(legacy_selected(&fake, &config()).is_err());
    for invalid in [r#"{"sandbox":"true"}"#, r#"{"timeoutSecs":-1}"#, r#"{"toolProfile":7}"#] {
        assert!(serde_json::from_str::<config::CompileConfig>(invalid).is_err());
    }
    for invalid in [r#"{"timeoutSecs":0}"#, r#"{"toolProfile":" "}"#] {
        assert!(serde_json::from_str::<config::CompileConfig>(invalid)?.validate().is_err());
    }
    Ok(())
}
#[test]
fn page_path_rejects_combined_parent_directory_traversal() -> anyhow::Result<()> {
    let root = Temp::new()?;
    let wiki = root.0.join("wiki");
    for path in [
        "a/../b",
        "概念/../../etc/passwd",
        "概念/嵌套/../穿透.md",
        r"a\..\b",
    ] {
        let error = progress::page_path(&wiki, path)
            .expect_err(&format!("parent directory traversal must be rejected: {path:?}"));
        assert!(
            error.to_string().starts_with("WikiWriteOutsideRoot:"),
            "unexpected rejection for {path:?}: {error}",
        );
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn symlink_escapes_and_dangling_links_fail_closed() -> anyhow::Result<()> {
    let root = Temp::new()?;
    let wiki = root.0.join("wiki");
    std::os::unix::fs::symlink(root.0.join("raw"), wiki.join("escape"))?;
    std::os::unix::fs::symlink(root.0.join("absent"), wiki.join("dangling"))?;
    for path in ["escape/source.md", "escape/new.md", "dangling/new.md"] {
        assert!(progress::page_path(&wiki, path).is_err());
    }
    Ok(())
}

#[test]
fn startup_and_terminal_failures_still_close_and_refresh_index() -> anyhow::Result<()> {
    let root = Temp::new()?;
    for startup in [true, false] {
        let fake = Fake::new(&root, vec![event("error", Some("provider failed".into()))], Some("Error"));
        let mut fake = Arc::try_unwrap(fake).map_err(|_| anyhow::anyhow!("fake still shared"))?;
        fake.fail_start = startup;
        let fake = Arc::new(fake);
        let events = run(fake.clone(), &root);
        assert_eq!(fake.closes.load(Ordering::SeqCst), 1);
        assert!(fake.aborts.load(Ordering::SeqCst) > 0);
        assert!(events.last().and_then(|e| e.text.as_ref()).is_some_and(|s| s.contains("中断")));
    }
    Ok(())
}

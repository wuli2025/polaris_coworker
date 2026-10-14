//! Bounded headless lifecycle; all failures reindex and emit exactly one done.
use super::{config::CompileConfig, progress::Progress, KbCompileEvent};
use polaris_kernel::chat::{PermissionMode, backend::{
    AgentBackend, TurnRequest, ToolOptions, EventStream, host_tools,
    pi::protocol::{Decision, PermissionResponse}, stream::StreamError,
}};
use std::{path::Path, sync::{Arc, mpsc}, time::{Duration, Instant}};

pub(super) fn run(
    backend: Arc<dyn AgentBackend>, req: &str, kb_root: &Path, config: &CompileConfig,
    mut emit: impl FnMut(KbCompileEvent), reindex: impl FnOnce() -> usize,
) {
    let mut count = 0;
    let result = (|| -> anyhow::Result<()> {
        let kb_root = std::fs::canonicalize(kb_root)?;
        let wiki = kb_root.join("wiki");
        std::fs::create_dir_all(&wiki)?;
        let wiki = std::fs::canonicalize(wiki)?;
        anyhow::ensure!(wiki.starts_with(&kb_root) && wiki != kb_root, "WikiRootOutsideKnowledgeBase");
        let mut input = TurnRequest::headless(req.into(), wiki.clone(),
            super::prompt::pi_directive(&kb_root.to_string_lossy()), PermissionMode::AutoCurrent);
        let timeout = Duration::from_secs(config.timeout_secs);
        input.timeout = Some(timeout);
        let mut progress = Progress::new(wiki);
        let options = ToolOptions { sandbox: config.sandbox, tool_profile: Some(config.tool_profile.clone()),
            detailed_events: true, ..Default::default() };
        // Separate timer covers blocking startup and sandbox execution as well as stream polling.
        let deadline = Instant::now() + timeout;
        let (finished, timer) = mpsc::channel::<()>();
        let timer_backend = backend.clone();
        let timer_req = req.to_owned();
        let watchdog = std::thread::spawn(move || {
            if matches!(timer.recv_timeout(timeout), Err(mpsc::RecvTimeoutError::Timeout)) {
                let _ = timer_backend.cancel(&timer_req);
            }
        });
        let result = match backend.start_with_tools(input, options) {
            Ok(stream) => {
                emit(event(req, "phase", "读取资料、抽取实体与概念…".into()));
                let outcome = consume(backend.as_ref(), &stream, deadline, &mut progress, |kind, text| {
                    emit(event(req, kind, text));
                });
                if outcome.is_err() {
                    let _ = backend.cancel(req);
                    // Wait for abort's terminal before close, with a bounded fallback.
                    drain(&stream, Duration::from_millis(config.abort_grace_ms));
                }
                count = progress.page_count();
                outcome
            }
            Err(error) => { let _ = backend.cancel(req); Err(error) }
        };
        let _ = finished.send(());
        let _ = watchdog.join();
        let closed = backend.close(req);
        result.and(closed)
    })();
    let n = reindex();
    if let Err(error) = &result { emit(event(req, "error", error.to_string())); }
    let text = if result.is_ok() {
        format!("编译完成: 新建/更新 {count} 个页面, 知识库共 {n} 篇")
    } else { "编译中断 (见上方原因), 已刷新索引".into() };
    emit(KbCompileEvent { run_id: req.into(), kind: "done".into(), text: Some(text), doc_count: Some(n) });
}
fn event(req: &str, kind: &str, text: String) -> KbCompileEvent {
    KbCompileEvent { run_id: req.into(), kind: kind.into(), text: Some(text), doc_count: None }
}
fn consume(
    backend: &dyn AgentBackend, stream: &EventStream, deadline: Instant, progress: &mut Progress,
    mut emit: impl FnMut(&str, String),
) -> anyhow::Result<()> {
    let mut error = None;
    loop {
        anyhow::ensure!(Instant::now() < deadline, "WikiCompileTimeout");
        let event = stream.recv_timeout(deadline.saturating_duration_since(Instant::now()))?;
        if host_tools::handle_sandbox(backend, &event)? { continue; }
        match event.kind.as_str() {
            "permission.request" => {
                let data: serde_json::Value = serde_json::from_str(event.text.as_deref().unwrap_or_default())?;
                let id = |name| data.get(name).and_then(serde_json::Value::as_str)
                    .map(str::to_owned).ok_or_else(|| anyhow::anyhow!("WikiInvalidPermissionRequest"));
                backend.permission_respond(PermissionResponse {
                    session_id: id("sessionId")?, call_id: id("callId")?, decision: Decision::Deny,
                })?;
                anyhow::bail!("WikiPermissionDenied: operation requires permission outside the wiki write scope");
            }
            "error" => {
                let message = event.text.unwrap_or_else(|| "WikiBackendError".into());
                emit("error", message.clone());
                if !message.starts_with("ToolFailed: ") { error = Some(message); }
            }
            "done" => {
                anyhow::ensure!(!matches!(event.text.as_deref(), Some("Aborted" | "Timeout" | "Error")),
                    "WikiCompileInterrupted: {}", event.text.as_deref().unwrap_or_default());
                return match error { Some(error) => Err(anyhow::anyhow!(error)), None => Ok(()) };
            }
            _ => if let Some((kind, text)) = progress.accept(&event)? { emit(kind, text); },
        }
    }
}
fn drain(stream: &EventStream, grace: Duration) {
    let until = Instant::now() + grace;
    while Instant::now() < until {
        match stream.recv_timeout(until.saturating_duration_since(Instant::now())) {
            Ok(event) if event.kind == "done" => break,
            Ok(_) => {},
            Err(error) if matches!(error.downcast_ref(), Some(StreamError::Closed)) => break,
            Err(_) => break,
        }
    }
}

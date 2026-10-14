//! UI delivery reuses the existing prompt assembly, artifact diff and persistence.
use super::*;
use crate::chat::ChatStreamEvent;
#[cfg(feature = "desktop")]
use tauri::Emitter;

pub(crate) fn run_chat(app: &AppHandle, req: &str, args: ChatSendArgs) -> Result<(), String> {
    let backend = selected().map_err(|e| e.to_string())?;
    let legacy = backend.status().backend == "claude-cli";
    let conv = args.conversation_id.clone();
    let permission = match args.permission_mode {
        PermissionMode::Manual => PermissionMode::Manual, PermissionMode::AutoAll => PermissionMode::AutoAll,
        PermissionMode::AutoCurrent => PermissionMode::AutoCurrent, PermissionMode::Deny => PermissionMode::Deny,
    };
    let mut input = TurnRequest {
        req_id: req.into(), session_id: conv.clone().unwrap_or_else(|| req.into()),
        conversation_id: conv.clone(), prompt: args.prompt.clone(), permission,
        model: None, cwd: claude_cli::pipeline::effective_cwd(None), attachments: vec![],
        account_id: None, timeout: None, legacy: None,
    };
    let mut snapshot = None;
    let mut notice = None;
    if legacy { input.legacy = Some((app.clone(), args)); }
    else {
        if args.use_sandbox { return Err(BackendError::UnsupportedInput("sandbox requires P3 tool routing").to_string()); }
        if args.provider_id.as_deref().is_some_and(|p| !p.is_empty() && p != "auto") {
            return Err(BackendError::UnsupportedInput("provider selection requires P4 mapping").to_string());
        }
        let prepared = claude_cli::pipeline::prepare(args);
        input.prompt = prepared.plan.full_text();
        input.cwd = claude_cli::pipeline::effective_cwd(prepared.project_work_dir.as_deref().map(std::path::Path::new));
        notice = prepared.image_notice;
        snapshot = Some((prepared.art_dir, prepared.art_before));
    }
    if crate::runtime::procs::CHILDREN.take_cancel(req) {
        emit(app, event(req, conv.as_deref(), "done", None));
        return Ok(());
    }
    let stream = backend.start(input).map_err(|e| e.to_string())?;
    let mut finalizer = None;
    let timeout = Duration::from_millis(config::Config::load().map_err(|e| e.to_string())?.agentd.request_timeout_ms);
    loop {
        let ev = match stream.recv_timeout(timeout) {
            Ok(event) => event,
            // Only the supervisor sees all agentd events (including hidden reasoning/usage).
            // Let it own turn deadlines and deliver the specific error followed by done.
            Err(error) if matches!(error.downcast_ref(), Some(stream::StreamError::Timeout)) => continue,
            Err(error) => { let _ = backend.cancel(req); return Err(error.to_string()); }
        };
        if ev.kind == "done" {
            if let (Some(finalizer), Some((artifacts, before))) = (finalizer.take(), snapshot.take()) {
                let finalizer: claude_cli::pipeline::TurnStream = finalizer;
                finalizer.finish(&artifacts, &before);
            } else { emit(app, ev); }
            if conv.is_none() { let _ = backend.close(req); }
            crate::runtime::procs::CHILDREN.take_cancel(req);
            break;
        }
        if ev.kind == "meta" && !legacy {
            emit(app, ev);
            finalizer = Some(claude_cli::pipeline::TurnStream::new(app.clone(), req.into(), conv.clone(), notice.take()));
            continue;
        }
        if let Some(finalizer) = &mut finalizer { finalizer.record_backend_event(&ev); }
        emit(app, ev);
    }
    Ok(())
}
pub(crate) fn emit(app: &AppHandle, event: ChatStreamEvent) { let _ = app.emit("chat:stream", event); }

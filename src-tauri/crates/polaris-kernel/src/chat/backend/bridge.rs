//! UI delivery reuses the existing prompt assembly, artifact diff and persistence.
use super::*;
use crate::chat::ChatStreamEvent;
#[cfg(feature = "desktop")]
use tauri::Emitter;

pub(crate) fn run_chat(app: &AppHandle, req: &str, args: ChatSendArgs) -> Result<(), String> {
    let backend = selected().map_err(|e| e.to_string())?;
    let legacy = backend.status().backend == "claude-cli";
    let conv = args.conversation_id.clone();
    let config = config::Config::load().map_err(|e| e.to_string())?;
    let mut input = chat_input::prepare(req, &args,
        &crate::chat::attach::uploads_dir(conv.as_deref()),
        claude_cli::pipeline::effective_cwd(None), &config.agentd.attachments)
        .map_err(|e| e.to_string())?;
    let mut tool_options = ToolOptions::default();
    let mut snapshot = None;
    let mut notice = None;
    if legacy {
        if args.use_sandbox { return Err("SANDBOX_UNAVAILABLE: legacy backend has no tool routing".into()); }
        input.legacy = Some((app.clone(), args));
    } else {
        if args.provider_id.as_deref().is_some_and(|p| !p.is_empty() && p != "auto") {
            return Err(BackendError::UnsupportedInput("provider selection requires P4 mapping").to_string());
        }
        tool_options = ToolOptions { sandbox: args.use_sandbox, expert_prompt: host_tools::expert_prompt(&args).map_err(|e| e.to_string())?, ..Default::default() };
        let prepared = claude_cli::pipeline::prepare_pi(args);
        input.model = claude_cli::pipeline::model_for_mode(prepared.work_full).map(serde_json::Value::String);
        input.prompt = prepared.plan.full_text();
        input.cwd = claude_cli::pipeline::effective_cwd(prepared.project_work_dir.as_deref().map(std::path::Path::new));
        notice = prepared.image_notice;
        snapshot = Some((prepared.art_dir, prepared.art_before));
    }
    if crate::runtime::procs::CHILDREN.take_cancel(req) {
        emit(app, event(req, conv.as_deref(), "done", None));
        return Ok(());
    }
    let stream = backend.start_with_tools(input, tool_options).map_err(|e| e.to_string())?;
    let mut finalizer = None;
    let timeout = Duration::from_millis(config.agentd.request_timeout_ms);
    loop {
        let ev = match stream.recv_timeout(timeout) {
            Ok(event) => event,
            // Only the supervisor sees all agentd events (including hidden reasoning/usage).
            // Let it own turn deadlines and deliver the specific error followed by done.
            Err(error) if matches!(error.downcast_ref(), Some(stream::StreamError::Timeout)) => continue,
            Err(error) => { let _ = backend.cancel(req); return Err(error.to_string()); }
        };
        if host_tools::handle_sandbox(backend.as_ref(), &ev).map_err(|e| e.to_string())? { continue; }
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
pub(crate) fn emit(app: &AppHandle, event: ChatStreamEvent) {
    if event.kind == "done" { crate::chat::live_input::finished(&event.req_id); }
    let _ = app.emit("chat:stream", event);
}

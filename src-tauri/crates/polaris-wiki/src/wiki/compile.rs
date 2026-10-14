//! Wiki compiler: shared maintenance lock, selected backend, unchanged kb:compile contract.
mod legacy;
mod prompt;
mod progress;
mod runner;
pub mod config;
#[cfg(test)]
mod tests;

use crate::kb::{acquire_kb_task, kb_reindex, kb_root_pathbuf, KB_COMPILE_COUNTER};
#[cfg(not(feature = "desktop"))]
use crate::host::AppHandle;
use polaris_kernel::chat::backend::{self, AgentBackend};
use serde::Serialize;
use std::sync::{Arc, atomic::Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
#[cfg(feature = "desktop")]
use tauri::{AppHandle, Emitter};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KbCompileEvent {
    pub run_id: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc_count: Option<usize>,
}
pub(crate) fn emit_compile(app: &AppHandle, run_id: &str, kind: &str, text: Option<String>) {
    let _ = app.emit("kb:compile", KbCompileEvent {
        run_id: run_id.into(), kind: kind.into(), text, doc_count: None,
    });
}
fn legacy_selected(backend: &dyn AgentBackend, config: &config::CompileConfig) -> anyhow::Result<bool> {
    match backend::config::BackendKind::parse(&backend.status().backend)? {
        backend::config::BackendKind::Pi => Ok(false),
        backend::config::BackendKind::ClaudeCli => {
            anyhow::ensure!(!config.sandbox, "SANDBOX_UNAVAILABLE: legacy wiki compiler has no sandbox routing");
            Ok(true)
        }
    }
}

/// Host callers can explicitly preserve their user's sandbox choice through this API.
pub fn compile_with_config(app: AppHandle, config: config::CompileConfig) -> Result<String, String> {
    config.validate().map_err(|e| e.to_string())?;
    let root = kb_root_pathbuf();
    if root.as_os_str().is_empty() || !root.is_dir() {
        return Err("知识库根目录不存在, 请先在「管理」里设置".into());
    }
    let backend = backend::selected().map_err(|e| e.to_string())?;
    let legacy = legacy_selected(backend.as_ref(), &config).map_err(|e| e.to_string())?;
    let guard = acquire_kb_task()?;
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let run_id = format!("kbc-{:x}-{:x}", ts, KB_COMPILE_COUNTER.fetch_add(1, Ordering::Relaxed));
    let rid = run_id.clone();
    std::thread::spawn(move || {
        let _guard = guard;
        emit_compile(&app, &rid, "phase", Some("启动 wiki 维护者…".into()));
        if legacy {
            legacy::run(app, rid, root, backend, config.timeout_secs);
        } else {
            runner::run(backend, &rid, &root, &config, |event| {
                let _ = app.emit("kb:compile", event);
            }, kb_reindex);
        }
    });
    Ok(run_id)
}
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn kb_compile(app: AppHandle) -> Result<String, String> {
    compile_with_config(app, config::CompileConfig::load().map_err(|e| e.to_string())?)
}
/// Host cancellation uses the same abort port as chat; no direct daemon/process kill.
pub fn cancel_compile(run_id: &str) -> Result<(), String> {
    let backend: Arc<dyn AgentBackend> = backend::selected().map_err(|e| e.to_string())?;
    backend.cancel(run_id).map_err(|e| e.to_string())
}

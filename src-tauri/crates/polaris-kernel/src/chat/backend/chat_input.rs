//! Chat boundary conversion shared by the live bridge and input behavior tests.
use std::path::{Path, PathBuf};
use super::{images, pi::protocol::ThinkingLevel, TurnRequest};
use crate::chat::{ChatSendArgs, PermissionMode};

/// `uploads` must be resolved by the host for this conversation, never from IPC.
/// Validation happens before prompt preparation or any backend request.
pub fn prepare(
    req: &str,
    args: &ChatSendArgs,
    uploads: &Path,
    cwd: PathBuf,
    limits: &images::ImageLimits,
) -> anyhow::Result<TurnRequest> {
    let thinking_level = args.thinking_level.as_deref().map(|level| {
        serde_json::from_value::<ThinkingLevel>(serde_json::Value::String(level.into()))
            .map_err(|_| anyhow::anyhow!("THINKING_LEVEL_INVALID: expected off/minimal/low/medium/high/xhigh"))
    }).transpose()?;
    let attachments = images::load(uploads, args.image_paths.as_deref().map_or(&[], |paths| paths), limits)?;
    let permission = match args.permission_mode {
        PermissionMode::Manual => PermissionMode::Manual,
        PermissionMode::AutoCurrent => PermissionMode::AutoCurrent,
        PermissionMode::AutoAll => PermissionMode::AutoAll,
        PermissionMode::Deny => PermissionMode::Deny,
    };
    Ok(TurnRequest {
        req_id: req.into(),
        session_id: args.conversation_id.clone().map_or_else(|| req.into(), |id| id),
        conversation_id: args.conversation_id.clone(),
        prompt: args.prompt.clone(), permission, model: None, cwd,
        attachments, thinking_level, account_id: None, timeout: None, legacy: None,
    })
}

#[cfg(test)]
mod tests;

//! Host input conversion; no provider aliases or attachment paths are guessed.
use super::{client::Client, protocol::{Attachment, Command, Empty, Model, SessionOptions}};
use super::super::{TurnRequest, PermissionMode};
use base64::Engine;
use std::{io::Read, path::PathBuf};

pub fn options(client: &Client, input: &TurnRequest) -> anyhow::Result<SessionOptions> {
    let model = match &input.model {
        None => None,
        Some(serde_json::Value::String(id)) => {
            // Existing POLARIS_*_MODEL values contain only an ID. Resolve it only
            // when the daemon catalog identifies exactly one provider.
            let catalog = client.call(Command::Models(Empty {}))?;
            let models: Vec<Model> = serde_json::from_value(catalog.get("models").cloned()
                .ok_or_else(|| anyhow::anyhow!("AgentdMissingModels"))?
                .as_array().ok_or_else(|| anyhow::anyhow!("AgentdInvalidModels"))?
                .iter().map(|m| serde_json::json!({"provider":m["provider"], "id":m["id"]})).collect::<Vec<_>>().into())?;
            let mut matches = models.into_iter().filter(|m| &m.id == id);
            let model = matches.next().ok_or_else(|| anyhow::anyhow!("AgentdModelNotFound: {id}"))?;
            anyhow::ensure!(matches.next().is_none(), "AgentdModelAmbiguous: specify provider and id");
            Some(model)
        }
        Some(value) => Some(serde_json::from_value::<Model>(value.clone())?),
    };
    if let Some(model) = &model {
        anyhow::ensure!(!model.provider.is_empty() && !model.id.is_empty(), "AgentdInvalidModel");
    }
    Ok(SessionOptions { model, thinking_level: None,
        tool_profile: Some(match input.permission { PermissionMode::Deny => "readonly", _ => "workspace" }.into()) })
}
pub fn attachments(paths: &[PathBuf], max_line_bytes: usize) -> anyhow::Result<Vec<Attachment>> {
    let mut total = 0;
    paths.iter().map(|path| {
        let mut bytes = Vec::new();
        std::fs::File::open(path)?.take(max_line_bytes as u64 + 1).read_to_end(&mut bytes)?;
        total += bytes.len();
        anyhow::ensure!(total <= max_line_bytes, "AgentdAttachmentSizeLimit");
        let mime = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") { "image/png" }
            else if bytes.starts_with(b"\xff\xd8\xff") { "image/jpeg" }
            else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") { "image/gif" }
            else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") { "image/webp" }
            else { anyhow::bail!("AgentdAttachmentTypeUnsupported"); };
        Ok(Attachment::Image { mime_type: mime.into(), data: base64::engine::general_purpose::STANDARD.encode(bytes) })
    }).collect()
}

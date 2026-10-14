//! Bounded image loading at the chat boundary. Paths are never inferred from text.
use std::{fs, io::Read, path::{Component, Path}};
use anyhow::Context;
use base64::Engine;
use serde::{Deserialize, Serialize};
use super::pi::protocol::Attachment;

const SUPPORTED: [&str; 4] = ["image/png", "image/jpeg", "image/gif", "image/webp"];

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ImageLimits {
    pub max_count: usize,
    pub max_bytes: usize,
    pub max_total_bytes: usize,
    pub mime_types: Vec<String>,
}
impl Default for ImageLimits {
    fn default() -> Self {
        Self { max_count: 4, max_bytes: 262_144, max_total_bytes: 524_288,
            mime_types: SUPPORTED.iter().map(|s| (*s).into()).collect() }
    }
}
impl ImageLimits {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(self.max_bytes > 0 && self.max_total_bytes > 0,
            "InvalidAgentConfig: attachments byte limits must be positive");
        anyhow::ensure!(self.mime_types.iter().all(|m| SUPPORTED.contains(&m.as_str())),
            "InvalidAgentConfig: unsupported attachments.mimeTypes");
        Ok(())
    }
}

/// `uploads` is resolved by the host from the conversation ID, never supplied by IPC.
pub fn load(uploads: &Path, paths: &[String], limits: &ImageLimits) -> anyhow::Result<Vec<Attachment>> {
    limits.validate()?;
    anyhow::ensure!(paths.len() <= limits.max_count, "ATTACHMENT_COUNT_LIMIT");
    if paths.is_empty() { return Ok(vec![]); }
    anyhow::ensure!(!fs::symlink_metadata(uploads)
        .context("ATTACHMENT_PATH_INVALID: uploads directory")?.file_type().is_symlink(),
        "ATTACHMENT_PATH_OUTSIDE_UPLOADS: uploads cannot be a symlink");
    let root = uploads.canonicalize().context("ATTACHMENT_PATH_INVALID: uploads directory")?;
    let mut total = 0usize;
    paths.iter().map(|raw| {
        let path = Path::new(raw);
        anyhow::ensure!(path.is_absolute() && !path.components().any(|c| c == Component::ParentDir),
            "ATTACHMENT_PATH_INVALID: absolute path without parent traversal required");
        let path = path.canonicalize().context("ATTACHMENT_PATH_INVALID: image path")?;
        anyhow::ensure!(path.starts_with(&root) && path != root, "ATTACHMENT_PATH_OUTSIDE_UPLOADS");
        anyhow::ensure!(path.is_file(), "ATTACHMENT_NOT_FILE");
        let mut options = fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        let file = options.open(&path).context("ATTACHMENT_READ_FAILED")?;
        let meta = file.metadata().context("ATTACHMENT_READ_FAILED")?;
        anyhow::ensure!(meta.is_file(), "ATTACHMENT_NOT_FILE");
        let remaining = limits.max_total_bytes.saturating_sub(total);
        let budget = limits.max_bytes.min(remaining);
        anyhow::ensure!(meta.len() <= budget as u64, "ATTACHMENT_SIZE_LIMIT");
        let mut bytes = Vec::new();
        file.take((budget as u64).saturating_add(1)).read_to_end(&mut bytes)
            .context("ATTACHMENT_READ_FAILED")?;
        anyhow::ensure!(bytes.len() <= budget, "ATTACHMENT_SIZE_LIMIT");
        total += bytes.len();
        let mime = detect_mime(&bytes).ok_or_else(|| anyhow::anyhow!("ATTACHMENT_TYPE_UNSUPPORTED"))?;
        anyhow::ensure!(limits.mime_types.iter().any(|m| m == mime), "ATTACHMENT_TYPE_UNSUPPORTED");
        Ok(Attachment::Image { mime_type: mime.into(),
            data: base64::engine::general_purpose::STANDARD.encode(bytes) })
    }).collect()
}

fn detect_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") { Some("image/png") }
    else if bytes.starts_with(b"\xff\xd8\xff") { Some("image/jpeg") }
    else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") { Some("image/gif") }
    else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") { Some("image/webp") }
    else { None }
}

#[cfg(test)]
mod tests;

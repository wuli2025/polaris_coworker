//! Translate only successful in-workspace writes into page progress.
use polaris_kernel::chat::ChatStreamEvent;
use serde_json::Value;
use std::{collections::{HashMap, HashSet}, path::{Component, Path, PathBuf}};

pub(super) struct Progress {
    root: PathBuf,
    pending: HashMap<String, PathBuf>,
    pages: HashSet<PathBuf>,
}
impl Progress {
    pub fn new(root: PathBuf) -> Self {
        Self { root, pending: HashMap::new(), pages: HashSet::new() }
    }
    pub fn page_count(&self) -> usize { self.pages.len() }
    pub fn accept(&mut self, event: &ChatStreamEvent) -> anyhow::Result<Option<(&'static str, String)>> {
        match event.kind.as_str() {
            "delta" => Ok(event.text.clone().map(|text| ("delta", text))),
            "tool.started" => {
                let data: Value = serde_json::from_str(event.text.as_deref().unwrap_or_default())?;
                let tool = string(&data, "tool")?;
                if matches!(tool, "write" | "edit") {
                    let path = page_path(&self.root, string(&data["args"], "path")?)?;
                    self.pending.insert(string(&data, "callId")?.into(), path);
                }
                Ok(Some(("tool", tool.into())))
            }
            "tool.completed" => {
                let data: Value = serde_json::from_str(event.text.as_deref().unwrap_or_default())?;
                let path = self.pending.remove(string(&data, "callId")?);
                if data.get("isError").and_then(Value::as_bool) == Some(false) {
                    if let Some(path) = path {
                        // Check again after execution (symlink/rename changes fail closed).
                        let path = page_path(&self.root, &path.to_string_lossy())?;
                        let relative = path.strip_prefix(&self.root)?.to_string_lossy().replace('\\', "/");
                        self.pages.insert(path);
                        return Ok(Some(("page", format!("写入 {relative}"))));
                    }
                }
                Ok(Some(("tool", format!("{} 完成", string(&data, "tool")?))))
            }
            _ => Ok(None),
        }
    }
}
fn string<'a>(value: &'a Value, key: &str) -> anyhow::Result<&'a str> {
    value.get(key).and_then(Value::as_str).ok_or_else(|| anyhow::anyhow!("InvalidCompileEvent: {key}"))
}
/// New files resolve through their nearest existing ancestor; dangling symlinks fail.
pub(super) fn page_path(root: &Path, input: &str) -> anyhow::Result<PathBuf> {
    let input = input.strip_prefix('@').unwrap_or(input);
    anyhow::ensure!(!input.is_empty() && !input.split(['/', '\\']).any(|p| p == ".."), "WikiWriteOutsideRoot: {input}");
    #[cfg(not(windows))]
    anyhow::ensure!(!input.contains('\\') && !input.contains(':'), "WikiWriteOutsideRoot: {input}");
    let path = Path::new(input);
    anyhow::ensure!(!path.components().any(|c| matches!(c, Component::ParentDir)), "WikiWriteOutsideRoot: {input}");
    let absolute = root.join(path);
    let resolved = canonical_new(&absolute)?;
    anyhow::ensure!(resolved.starts_with(root) && resolved != root, "WikiWriteOutsideRoot: {input}");
    Ok(resolved)
}
fn canonical_new(path: &Path) -> anyhow::Result<PathBuf> {
    match std::fs::canonicalize(path) {
        Ok(path) => Ok(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            anyhow::ensure!(std::fs::symlink_metadata(path).is_err(), "WikiUnresolvedSymlink");
            let parent = path.parent().ok_or_else(|| anyhow::anyhow!("WikiInvalidPath"))?;
            let name = path.file_name().ok_or_else(|| anyhow::anyhow!("WikiInvalidPath"))?;
            Ok(canonical_new(parent)?.join(name))
        }
        Err(error) => Err(error.into()),
    }
}

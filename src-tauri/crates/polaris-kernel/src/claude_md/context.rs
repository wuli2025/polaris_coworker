//! Backend-neutral editor contract. A selection pins both the cwd and filename.
use serde::{Deserialize, Serialize};
use std::{fs, io, path::{Component, Path, PathBuf}};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Selection {
    pub root: String,
    pub file_name: String,
    pub relative_path: String,
    pub agents_exists: bool,
    pub claude_exists: bool,
    pub exists: bool,
}

fn io_error(error: io::Error) -> String {
    let name = match error.kind() {
        io::ErrorKind::NotFound => "ContextNotFound",
        io::ErrorKind::PermissionDenied => "ContextPermissionDenied",
        _ => "ContextIoFailed",
    };
    format!("{name}: {error}")
}

fn filename(name: &str) -> Result<(), String> {
    if matches!(name, "AGENTS.md" | "CLAUDE.md") { Ok(()) }
    else { Err("ContextPathRejected: expected AGENTS.md or CLAUDE.md".into()) }
}

fn canonical_root(root: &Path) -> Result<PathBuf, String> {
    if root.components().any(|c| c == Component::ParentDir) {
        return Err("ContextPathRejected: parent component".into());
    }
    let root = root.canonicalize().map_err(io_error)?;
    if !root.is_dir() { return Err("ContextPathRejected: root is not a directory".into()); }
    Ok(root)
}

fn target(root: &Path, name: &str) -> Result<(PathBuf, bool), String> {
    filename(name)?;
    let path = root.join(name);
    match fs::symlink_metadata(&path) {
        Ok(meta) => {
            // Reject even dangling links: absence must never authorize following a link on write.
            if meta.file_type().is_symlink() || !meta.is_file() {
                return Err("ContextPathRejected: context must be a regular file".into());
            }
            let canonical = path.canonicalize().map_err(io_error)?;
            if !crate::runtime::paths::path_contains(root, &canonical) {
                return Err("ContextPathRejected: outside project".into());
            }
            Ok((canonical, true))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok((path, false)),
        Err(e) => Err(io_error(e)),
    }
}

pub fn select(root: &Path, create_file: &str) -> Result<Selection, String> {
    filename(create_file)?;
    let root = canonical_root(root)?;
    let (_, agents_exists) = target(&root, "AGENTS.md")?;
    let (_, claude_exists) = target(&root, "CLAUDE.md")?;
    let name = if agents_exists { "AGENTS.md" } else if claude_exists { "CLAUDE.md" } else { create_file };
    Ok(Selection { root: root.to_string_lossy().into_owned(), file_name: name.into(),
        relative_path: name.into(), agents_exists, claude_exists, exists: agents_exists || claude_exists })
}

fn pinned(root: &Path, selection: &Selection) -> Result<PathBuf, String> {
    let root = canonical_root(root)?;
    if root.to_string_lossy() != selection.root || selection.relative_path != selection.file_name {
        return Err("ContextSelectionStale: project directory or selection changed".into());
    }
    target(&root, &selection.file_name).map(|(path, _)| path)
}

pub fn read(root: &Path, selection: &Selection) -> Result<String, String> {
    fs::read_to_string(pinned(root, selection)?).map_err(io_error)
}

pub fn write(root: &Path, selection: &Selection, content: &str) -> Result<(), String> {
    let path = pinned(root, selection)?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    use std::io::Write;
    options.open(path).map_err(io_error)?.write_all(content.as_bytes()).map_err(io_error)
}

fn project_directory(project_id: &str) -> Result<PathBuf, String> {
    if !crate::conv::list_active_projects().iter().any(|p| p.id == project_id) {
        return Err("ContextProjectNotFound".into());
    }
    let work_dir = crate::conv::project_work_dir(project_id);
    // Same resolver as actual chat dispatch, including the development/installed fallback.
    Ok(crate::chat::backend::claude_cli::pipeline::effective_cwd(work_dir.as_deref().map(Path::new)))
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub fn context_file_metadata(project_id: String, create_file: Option<String>) -> Result<Selection, String> {
    select(&project_directory(&project_id)?, create_file.as_deref().unwrap_or("AGENTS.md"))
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub fn context_file_read(project_id: String, selection: Selection) -> Result<String, String> {
    read(&project_directory(&project_id)?, &selection)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub fn context_file_write(project_id: String, selection: Selection, content: String) -> Result<(), String> {
    write(&project_directory(&project_id)?, &selection, &content)
}

#[cfg(test)]
#[path = "context_tests.rs"]
mod tests;

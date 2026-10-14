//! Pi shell selection shared by spawning and doctor. No process-global PATH writes.
use crate::chat::backend::config::Agentd;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShellSource {
    #[cfg(any(windows, test))]
    Bundled,
    #[cfg(any(windows, test))]
    ProgramFiles,
    Path,
    Explicit,
}

impl ShellSource {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            #[cfg(any(windows, test))]
            Self::Bundled => "bundled",
            #[cfg(any(windows, test))]
            Self::ProgramFiles => "program_files",
            Self::Path => "path",
            Self::Explicit => "explicit",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedShell {
    pub(crate) path: PathBuf,
    pub(crate) source: ShellSource,
}

fn configured_shell(config: &Agentd) -> Result<Option<PathBuf>, String> {
    let file = if config.args.first().is_some_and(|s| s == "--config") {
        config.args.get(1).map(PathBuf::from)
    } else {
        std::env::var_os("POLARIS_AGENTD_CONFIG").map(PathBuf::from)
    };
    let explicit = if let Some(file) = file {
        let bytes = std::fs::read(file).map_err(|_| "AgentdConfigUnreadable")?;
        let json: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| "AgentdConfigInvalid")?;
        match json.get("shellPath") {
            None | Some(serde_json::Value::Null) => None,
            Some(serde_json::Value::String(s)) => (!s.is_empty()).then(|| s.clone()),
            _ => return Err("AgentdShellPathInvalid".into()),
        }
    } else { None };
    let value = explicit.or_else(|| std::env::var("POLARIS_AGENTD_SHELL_PATH").ok().filter(|s| !s.is_empty()));
    value.map(|value| {
        if value == "~" || value.starts_with("~/") || value.starts_with("~\\") {
            super::probe::home_dir().map(|home| home.join(value.get(2..).map_or("", |s| s)))
                .ok_or_else(|| "AgentdShellHomeMissing".into())
        } else { Ok(PathBuf::from(value)) }
    }).transpose()
}

#[cfg(any(windows, test))]
fn windows_components(path: &Path) -> Vec<String> {
    let normalized = path.to_string_lossy().replace('\\', "/").to_ascii_lowercase();
    let mut parts = Vec::new();
    for part in normalized.trim_start_matches("//?/").split('/') {
        match part {
            "" | "." => {}
            ".." => { parts.pop(); }
            _ => parts.push(part.to_owned()),
        }
    }
    parts
}

#[cfg(any(windows, test))]
fn excluded_windows_shell(path: &Path, system_root: &Path) -> bool {
    let parts = windows_components(path);
    let root = windows_components(system_root);
    parts.iter().any(|part| part == "windowsapps")
        || (!root.is_empty() && parts.starts_with(&root))
}

#[cfg(any(windows, test))]
fn windows_shell(
    bundled: Option<PathBuf>, program_files: impl IntoIterator<Item = PathBuf>,
    on_path: impl FnOnce() -> Vec<PathBuf>, system_root: &Path,
) -> Option<ResolvedShell> {
    bundled.map(|path| ResolvedShell { path, source: ShellSource::Bundled })
        .or_else(|| program_files.into_iter().map(|p| p.join("Git/bin/bash.exe"))
            .find(|p| p.is_file())
            .map(|path| ResolvedShell { path, source: ShellSource::ProgramFiles }))
        .or_else(|| on_path().into_iter()
            .find(|p| !excluded_windows_shell(p, system_root) && p.is_file())
            .map(|path| ResolvedShell { path, source: ShellSource::Path }))
}

pub(crate) fn resolve(config: &Agentd) -> Result<Option<ResolvedShell>, String> {
    if let Some(explicit) = configured_shell(config)? {
        // Pi rejects an invalid explicit shell instead of silently falling back.
        return Ok(explicit.is_file().then_some(ResolvedShell { path: explicit, source: ShellSource::Explicit }));
    }
    #[cfg(windows)]
    {
        let system_root = ["SystemRoot", "windir"].into_iter()
            .filter_map(std::env::var_os).find(|value| !value.is_empty())
            .map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
        Ok(windows_shell(super::bundled::git_bash(),
        ["ProgramFiles", "ProgramFiles(x86)"].into_iter().filter_map(std::env::var_os).map(PathBuf::from),
        || super::probe::which_all("bash.exe"), &system_root))
    }
    #[cfg(not(windows))]
    { Ok(if Path::new("/bin/bash").is_file() { Some("/bin/bash".into()) }
        else { super::probe::which_all("bash").into_iter().next()
            .or_else(|| super::probe::which_all("sh").into_iter().next()) }
        .map(|path| ResolvedShell { path, source: ShellSource::Path })) }
}

#[cfg(any(windows, test))]
fn apply_environment(
    command: &mut std::process::Command, shell: &Path, prefix: Vec<PathBuf>,
    current_path: Option<std::ffi::OsString>,
) -> Result<(), String> {
    command.env("POLARIS_AGENTD_SHELL_PATH", shell);
    if !prefix.is_empty() {
        let mut paths = prefix.clone();
        if let Some(current) = current_path {
            paths.extend(std::env::split_paths(&current).filter(|p| !prefix.contains(p)));
        }
        let path = std::env::join_paths(paths).map_err(|_| "AgentdShellPathJoinFailed")?;
        command.env("PATH", path);
    }
    Ok(())
}

#[cfg(windows)]
pub(crate) fn configure(command: &mut std::process::Command, config: &Agentd) -> Result<(), String> {
    let shell = resolve(config)?.ok_or("AgentShellMissing")?;
    // An explicit path may point to the bundle and still need its Unix utilities.
    let bundled = super::bundled::git_bash().as_deref() == Some(shell.path.as_path());
    apply_environment(command, &shell.path, super::bundled::child_path_prefix(bundled), std::env::var_os("PATH"))
}

#[cfg(test)]
#[path = "pi_shell_tests.rs"]
mod tests;

//! Pi shell selection shared by spawning and doctor. No process-global PATH writes.
use crate::chat::backend::config::Agentd;
use std::path::{Path, PathBuf};

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
fn windows_shell(
    bundled: Option<PathBuf>, program_files: impl IntoIterator<Item = PathBuf>,
    on_path: impl FnOnce() -> Option<PathBuf>,
) -> Option<PathBuf> {
    bundled.or_else(|| program_files.into_iter().map(|p| p.join("Git/bin/bash.exe"))
        .find(|p| p.is_file())).or_else(|| on_path().filter(|p| p.is_file()))
}

pub(crate) fn resolve(config: &Agentd) -> Result<Option<PathBuf>, String> {
    if let Some(explicit) = configured_shell(config)? {
        // Pi rejects an invalid explicit shell instead of silently falling back.
        return Ok(explicit.is_file().then_some(explicit));
    }
    #[cfg(windows)]
    { Ok(windows_shell(super::bundled::git_bash(),
        ["ProgramFiles", "ProgramFiles(x86)"].into_iter().filter_map(std::env::var_os).map(PathBuf::from),
        || super::probe::which_all("bash.exe").into_iter().next())) }
    #[cfg(not(windows))]
    { Ok(if Path::new("/bin/bash").is_file() { Some("/bin/bash".into()) }
        else { super::probe::which_all("bash").into_iter().next()
            .or_else(|| super::probe::which_all("sh").into_iter().next()) }) }
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
    if let Some(shell) = resolve(config)? {
        let bundled = super::bundled::git_bash().as_deref() == Some(shell.as_path());
        apply_environment(command, &shell, super::bundled::child_path_prefix(bundled), std::env::var_os("PATH"))?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "pi_shell_tests.rs"]
mod tests;

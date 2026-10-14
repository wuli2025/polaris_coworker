//! Public policy used both before docker run and before every routed tool call.
use crate::config::Config;
use anyhow::{bail, ensure, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub const REBUILD_HINT: &str = "请调用 sandbox_start({projectDir:目标项目路径,rebuild:true}) 迁移；旧容器将停止并改名保留，不会删除。";

pub fn path_for_docker(path: &Path) -> String {
    let text = path.to_string_lossy();
    let text = text.trim_start_matches(r"\\?\");
    // Detect drive paths too, so Windows fixtures can be verified on Unix.
    if cfg!(windows) || (text.as_bytes().get(1) == Some(&b':') && text.as_bytes()[0].is_ascii_alphabetic()) {
        text.replace('\\', "/")
    } else { text.to_owned() }
}
fn forbidden_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    ["data", "claude-home", "sessions", "conversations", "providers.json", "collab.db", ".polaris", ".pi",
        ".claude", ".codex", ".ssh", ".aws", ".gnupg", ".kube"].contains(&name.as_str()) || name.ends_with(".key")
}
fn normalized(path: &Path) -> Result<PathBuf> {
    // Resolve existing ancestors even for currently absent protected directories.
    if path.exists() { return Ok(std::fs::canonicalize(path)?); }
    let parent = path.parent().ok_or_else(|| anyhow::anyhow!("SANDBOX_INVALID_PATH"))?;
    let name = path.file_name().ok_or_else(|| anyhow::anyhow!("SANDBOX_INVALID_PATH"))?;
    Ok(normalized(parent)?.join(name))
}
pub fn validate_source(source: &Path, protected: &[PathBuf]) -> Result<PathBuf> {
    ensure!(source.is_absolute(), "SANDBOX_UNSAFE_MOUNT: absolute path required; {REBUILD_HINT}");
    let source = std::fs::canonicalize(source)?;
    for path in protected {
        let path = normalized(path)?;
        ensure!(!source.starts_with(&path) && !path.starts_with(&source),
            "SANDBOX_UNSAFE_MOUNT: protected host data; {REBUILD_HINT}");
    }
    ensure!(!source.components().any(|p| forbidden_name(&p.as_os_str().to_string_lossy())),
        "SANDBOX_UNSAFE_MOUNT: forbidden path; {REBUILD_HINT}");
    ensure!(source.is_dir(), "SANDBOX_UNSAFE_MOUNT: directory required");
    // Binding a parent also exposes its children. Reject sensitive files anywhere in it.
    let mut pending = vec![source.clone()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(directory)? {
            let entry = entry?;
            ensure!(!forbidden_name(&entry.file_name().to_string_lossy()),
                "SANDBOX_UNSAFE_MOUNT: directory contains protected files; {REBUILD_HINT}");
            // Symlinks cannot expose an unmounted host target inside the container.
            if entry.file_type()?.is_dir() { pending.push(entry.path()); }
        }
    }
    Ok(source)
}
#[derive(Debug)]
pub struct ResolvedMount { pub source: PathBuf, pub destination: String, pub read_only: bool }
impl Config {
    pub fn resolve_mounts(&self, project: &Path, kb: &Path, home: &Path) -> Result<Vec<ResolvedMount>> {
        self.validate()?;
        let protected = self.protected(home);
        let project = validate_source(project, &protected)?;
        let kb = validate_source(kb, &protected)?;
        ensure!(!project.starts_with(&kb) && !kb.starts_with(&project),
            "SANDBOX_UNSAFE_MOUNT: workspace and kb must not overlap");
        self.mounts.iter().map(|m| {
            let source = match m.source.as_str() {
                "${project}" => project.clone(), "${kb}" => kb.clone(),
                path => validate_source(Path::new(path), &protected)?,
            };
            let expected = if m.destination == "/workspace" { &project } else { &kb };
            ensure!(&source == expected, "SANDBOX_UNSAFE_MOUNT: source does not match requested root");
            Ok(ResolvedMount { source, destination: m.destination.clone(), read_only: m.read_only })
        }).collect()
    }
    pub fn run_args(&self, name: &str, image: &str, project: &Path, kb: &Path, home: &Path) -> Result<Vec<String>> {
        let mounts = self.resolve_mounts(project, kb, home)?;
        let mut args = vec!["run".into(), "-d".into(), "--name".into(), name.into(),
            format!("--memory={}", self.memory_bytes), format!("--cpus={}", self.cpus),
            "--cap-drop=ALL".into(), "--security-opt=no-new-privileges".into()];
        for capability in &self.cap_add { args.push(format!("--cap-add={capability}")); }
        for mount in mounts {
            let source = path_for_docker(&mount.source);
            ensure!(!source.contains([',', '\n', '\r']), "SANDBOX_INVALID_PATH: mount delimiter");
            args.push("--mount".into());
            args.push(format!("type=bind,src={source},dst={}{}", mount.destination,
                if mount.read_only { ",readonly" } else { "" }));
        }
        args.extend([image.into(), "sleep".into(), "infinity".into()]);
        Ok(args)
    }
    pub fn validate_container(&self, info: &Value, project: &Path, kb: &Path, home: &Path) -> Result<PathBuf> {
        let expected = self.resolve_mounts(project, kb, home)?;
        let mounts = info["Mounts"].as_array().ok_or_else(|| anyhow::anyhow!("SANDBOX_INVALID_MOUNTS"))?;
        // Check all sources first: legacy broad mounts always receive the migration diagnostic.
        for mount in mounts {
            ensure!(mount["Type"] == "bind", "SANDBOX_UNSAFE_MOUNT: unverified volume; {REBUILD_HINT}");
            let source = mount["Source"].as_str().ok_or_else(|| anyhow::anyhow!("SANDBOX_INVALID_MOUNTS"))?;
            validate_source(Path::new(source), &self.protected(home))?;
        }
        ensure!(mounts.len() == expected.len(), "SANDBOX_UNSAFE_MOUNT: unexpected mounts; {REBUILD_HINT}");
        let mut workdir_changed = false;
        for required in expected {
            let matching: Vec<_> = mounts.iter().filter(|m| m["Destination"] == required.destination).collect();
            ensure!(matching.len() == 1, "SANDBOX_UNSAFE_MOUNT: incorrect destination; {REBUILD_HINT}");
            let mount = matching[0];
            ensure!(mount["RW"].as_bool() == Some(!required.read_only),
                "SANDBOX_UNSAFE_MOUNT: incorrect access mode; {REBUILD_HINT}");
            let source = mount["Source"].as_str().ok_or_else(|| anyhow::anyhow!("SANDBOX_INVALID_MOUNTS"))?;
            if std::fs::canonicalize(source)? != required.source {
                if required.destination == "/workspace" { workdir_changed = true; }
                else { bail!("SANDBOX_UNSAFE_MOUNT: incorrect KB source; {REBUILD_HINT}"); }
            }
        }
        self.validate_security(info)?;
        ensure!(!workdir_changed, "SANDBOX_PATH_UNMOUNTED: work directory changed; {REBUILD_HINT}");
        Ok(std::fs::canonicalize(project)?)
    }
    pub fn validate_security(&self, info: &Value) -> Result<()> {
        let host = &info["HostConfig"];
        let memory = host["Memory"].as_u64().filter(|n| *n > 0 && *n <= self.memory_bytes).is_some();
        let cpu = host["NanoCpus"].as_u64().filter(|n| *n > 0 && *n <= (self.cpus * 1e9) as u64).is_some();
        let drop_all = host["CapDrop"].as_array().is_some_and(|a| a.iter().any(|v| v == "ALL"));
        let additions = host["CapAdd"].is_null() || host["CapAdd"].as_array().is_some_and(|a|
            a.iter().all(|v| v.as_str().is_some_and(|s| self.cap_add.iter().any(|c| c == s))));
        let no_new_privileges = host["SecurityOpt"].as_array().is_some_and(|a|
            a.iter().any(|v| v == "no-new-privileges" || v == "no-new-privileges:true"));
        ensure!(host["Privileged"] == false && host["PidMode"] != "host"
            && host["IpcMode"] != "host" && host["NetworkMode"] != "host"
            && host["Devices"].as_array().is_none_or(|a| a.is_empty())
            && memory && cpu && drop_all && additions && no_new_privileges,
            "SANDBOX_UNSAFE_CONTAINER: security baseline mismatch; {REBUILD_HINT}");
        Ok(())
    }
}

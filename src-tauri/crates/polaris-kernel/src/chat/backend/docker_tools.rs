//! Tool-only execution in an already managed container. Never spawns a host shell.
use super::host_tools::SandboxExecutor;
use serde_json::Value;
use std::{path::{Path, PathBuf}, process::Command};

pub struct DockerTools {
    pub container: String,
    pub host_root: PathBuf,
    pub guest_root: String,
}
/// Reuses a container managed by polaris-sandbox, inspecting its actual mounts.
/// Old containers mounting all of ~/Polaris are rejected because they expose keys.
pub struct ManagedSandbox {
    pub container: String,
    pub protected_paths: Vec<PathBuf>,
}
impl ManagedSandbox {
    pub fn route(&self, mounts: &Value, cwd: &Path) -> anyhow::Result<DockerTools> {
        let mounts = mounts.as_array().ok_or_else(|| anyhow::anyhow!("SANDBOX_INVALID_MOUNTS"))?;
        let cwd = std::fs::canonicalize(cwd)?;
        let mut selected: Option<(PathBuf, String)> = None;
        for mount in mounts {
            anyhow::ensure!(mount["Type"] == "bind", "SANDBOX_UNVERIFIED_VOLUME");
            let source = std::fs::canonicalize(text(mount, "Source")?)?;
            let destination = text(mount, "Destination")?;
            for protected in &self.protected_paths {
                let protected = std::fs::canonicalize(protected).unwrap_or_else(|_| protected.clone());
                anyhow::ensure!(!protected.starts_with(&source) && !source.starts_with(&protected),
                    "SANDBOX_UNSAFE_MOUNT: container exposes protected host data");
            }
            anyhow::ensure!(!source.starts_with("/var/run") && !source.starts_with("/run")
                && !source.starts_with("/proc") && !source.starts_with("/sys") && !source.starts_with("/dev"),
                "SANDBOX_UNSAFE_MOUNT: host control endpoint");
            if cwd.starts_with(&source) && mount["RW"] == true
                && selected.as_ref().is_none_or(|(previous, _)| source.components().count() > previous.components().count()) {
                selected = Some((source, destination.into()));
            }
        }
        let (host_root, guest_root) = selected.ok_or_else(|| anyhow::anyhow!("SANDBOX_PATH_UNMOUNTED"))?;
        Ok(DockerTools { container: self.container.clone(), host_root, guest_root })
    }
}
impl SandboxExecutor for ManagedSandbox {
    fn execute(&self, operation: &str, args: &Value) -> anyhow::Result<String> {
        let output = Command::new("docker").args(["inspect", &self.container]).output()
            .map_err(|e| anyhow::anyhow!("SANDBOX_UNAVAILABLE: {e}"))?;
        anyhow::ensure!(output.status.success(), "SANDBOX_UNAVAILABLE: managed container not found");
        let info: Vec<Value> = serde_json::from_slice(&output.stdout)?;
        let info = info.first().ok_or_else(|| anyhow::anyhow!("SANDBOX_INVALID_CONTAINER"))?;
        anyhow::ensure!(info["State"]["Running"] == true, "SANDBOX_UNAVAILABLE: container is stopped");
        anyhow::ensure!(info["HostConfig"]["Privileged"] != true
            && info["HostConfig"]["PidMode"] != "host"
            && info["HostConfig"]["NetworkMode"] != "host", "SANDBOX_UNSAFE_CONTAINER");
        self.route(&info["Mounts"], Path::new(text(args, "cwd")?))?.execute(operation, args)
    }
}
fn quote(value: &str) -> String { format!("'{}'", value.replace('\'', "'\\''")) }
fn text<'a>(args: &'a Value, key: &str) -> anyhow::Result<&'a str> {
    args.get(key).and_then(Value::as_str).ok_or_else(|| anyhow::anyhow!("SandboxMissingArgument: {key}"))
}
impl DockerTools {
    fn map_path(&self, path: &Path) -> anyhow::Result<String> {
        // Absolute host paths are only accepted under the existing managed mount.
        let root = std::fs::canonicalize(&self.host_root)?;
        let relative = path.strip_prefix(&root).map_err(|_| anyhow::anyhow!("SANDBOX_PATH_UNMOUNTED: {}", path.display()))?;
        anyhow::ensure!(!relative.components().any(|c| matches!(c, std::path::Component::ParentDir)), "SANDBOX_PATH_ESCAPE");
        Ok(format!("{}/{}", self.guest_root.trim_end_matches('/'), relative.to_string_lossy().replace('\\', "/")))
    }
    pub fn script(&self, operation: &str, args: &Value) -> anyhow::Result<(String, String)> {
        let cwd = self.map_path(Path::new(text(args, "cwd")?))?;
        let path = if operation == "bash" { String::new() } else {
            self.map_path(Path::new(text(args, "path")?))?
        };
        let path = quote(&path);
        let script = match operation {
            "bash" => text(args, "command")?.to_owned(),
            "mkdir" => format!("mkdir -p -- {path}"),
            "access" => format!("test -r {path} && test -w {path}"),
            "read" => format!("base64 < {path}"),
            "write" => {
                use base64::Engine;
                let content = base64::engine::general_purpose::STANDARD.encode(text(args, "content")?);
                format!("printf %s {} | base64 -d > {path}", quote(&content))
            }
            _ => anyhow::bail!("SANDBOX_OPERATION_UNSUPPORTED"),
        };
        Ok((cwd, script))
    }
}
impl SandboxExecutor for DockerTools {
    fn execute(&self, operation: &str, args: &Value) -> anyhow::Result<String> {
        let (cwd, script) = self.script(operation, args)?;
        let timeout_ms = args.get("timeoutMs").and_then(Value::as_u64)
            .filter(|n| *n > 0 && *n <= 2_147_483_647).ok_or_else(|| anyhow::anyhow!("SandboxInvalidTimeout"))?;
        let requested = args.get("timeout").and_then(Value::as_f64);
        let seconds = requested.filter(|n| n.is_finite() && *n > 0.0)
            .map_or(timeout_ms.div_ceil(1000), |n| (n.ceil() as u64).min(timeout_ms.div_ceil(1000)));
        let shell = text(args, "shell")?;
        anyhow::ensure!(["sh", "bash"].contains(&shell), "SandboxInvalidShell");
        let limit = args.get("maxOutputBytes").and_then(Value::as_u64)
            .filter(|n| *n > 0 && *n <= 1_048_576).ok_or_else(|| anyhow::anyhow!("SandboxInvalidOutputLimit"))?;
        // No -e flags: model keys and the agentd environment stay on the host.
        // Container timeout also applies after a lost daemon or cancelled host turn.
        let output = Command::new("docker").args(["exec", "-w", &cwd, &self.container,
            "timeout", "-s", "KILL", &seconds.to_string(), shell, "-c", &script]).output()
            .map_err(|e| anyhow::anyhow!("SANDBOX_UNAVAILABLE: {e}"))?;
        anyhow::ensure!(output.stdout.len() as u64 <= limit && output.stderr.len() as u64 <= limit, "SANDBOX_OUTPUT_LIMIT");
        anyhow::ensure!(output.status.success(), "SANDBOX_EXEC_FAILED: {}", String::from_utf8_lossy(&output.stderr));
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maps_mount_and_quotes_file_content_without_host_execution() -> anyhow::Result<()> {
        let root = std::fs::canonicalize(std::env::temp_dir())?;
        let executor = DockerTools { container: "test".into(), host_root: root.clone(), guest_root: "/workspace".into() };
        let args = serde_json::json!({"cwd":root,"path":root.join("file with 'quote"),"content":"$(touch /host)"});
        let (cwd, script) = executor.script("write", &args)?;
        assert_eq!(cwd, "/workspace/");
        assert!(!script.contains("$(touch"));
        assert!(script.contains("'\\''"));
        assert!(executor.map_path(&root.join("../outside")).is_err());
        Ok(())
    }
}

#[cfg(test)]
mod mount_tests {
    use super::*;
    #[test]
    fn old_whole_app_mount_is_rejected_and_project_mount_is_routed() -> anyhow::Result<()> {
        let base = std::env::temp_dir().join(format!("sandbox-mount-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_nanos()));
        let project = base.join("project");
        std::fs::create_dir_all(&project)?;
        let root = std::fs::canonicalize(&base)?;
        let managed = ManagedSandbox { container: "managed".into(), protected_paths: vec![root.join("data/providers.json")] };
        let old = serde_json::json!([{"Type":"bind","Source":root,"Destination":"/workspace","RW":true}]);
        assert!(managed.route(&old, &project).err().is_some_and(|e| e.to_string().contains("SANDBOX_UNSAFE_MOUNT")));
        let safe = serde_json::json!([{"Type":"bind","Source":project,"Destination":"/workspace","RW":true}]);
        let route = managed.route(&safe, &project)?;
        assert_eq!(route.host_root, std::fs::canonicalize(&project)?);
        assert_eq!(route.guest_root, "/workspace");
        std::fs::remove_dir_all(base)?;
        Ok(())
    }
}

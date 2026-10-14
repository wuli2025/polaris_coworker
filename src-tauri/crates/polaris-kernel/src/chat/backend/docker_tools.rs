//! Tool-only execution in an already managed container. Never spawns a host shell.
use super::{host_tools::{SandboxExecutor, SandboxOutput}, sandbox_process};
use std::time::{Duration, Instant};
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
    fn execute(&self, operation: &str, args: &Value) -> anyhow::Result<SandboxOutput> {
        let start = Instant::now();
        let budget = timeout(args)?;
        let output = sandbox_process::run(Command::new("docker").args(["inspect", &self.container]), budget, output_limit(args)?)?;
        anyhow::ensure!(output.exit_code == Some(0) && !output.timed_out, "SANDBOX_UNAVAILABLE: managed container not found or inspect timed out");
        let info: Vec<Value> = serde_json::from_str(&output.output)?;
        let info = info.first().ok_or_else(|| anyhow::anyhow!("SANDBOX_INVALID_CONTAINER"))?;
        anyhow::ensure!(info["State"]["Running"] == true, "SANDBOX_UNAVAILABLE: container is stopped");
        anyhow::ensure!(info["HostConfig"]["Privileged"] != true
            && info["HostConfig"]["PidMode"] != "host"
            && info["HostConfig"]["NetworkMode"] != "host", "SANDBOX_UNSAFE_CONTAINER");
        let mut remaining = args.clone();
        let left = budget.saturating_sub(start.elapsed()).as_millis();
        anyhow::ensure!(left > 0, "SANDBOX_UNAVAILABLE: probe budget exhausted");
        remaining["timeoutMs"] = serde_json::json!(left);
        self.route(&info["Mounts"], Path::new(text(args, "cwd")?))?.execute(operation, &remaining)
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
        let path = if ["bash", "probe"].contains(&operation) { String::new() } else {
            self.map_path(Path::new(text(args, "path")?))?
        };
        let path = quote(&path);
        let script = match operation {
            "probe" => "command -v setsid >/dev/null && command -v mktemp >/dev/null && command -v sleep >/dev/null".into(),
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
fn timeout(args: &Value) -> anyhow::Result<Duration> {
    args.get("timeoutMs").and_then(Value::as_u64).filter(|n| *n > 0 && *n <= 2_147_483_647)
        .map(Duration::from_millis).ok_or_else(|| anyhow::anyhow!("SandboxInvalidTimeout"))
}
fn output_limit(args: &Value) -> anyhow::Result<usize> {
    args.get("maxOutputBytes").and_then(Value::as_u64).filter(|n| *n > 0 && *n <= 1_048_576)
        .map(|n| n as usize).ok_or_else(|| anyhow::anyhow!("SandboxInvalidOutputLimit"))
}
const SUPERVISOR: &str = include_str!("sandbox_command.sh");
fn decode(mut output: SandboxOutput, marker: &str) -> anyhow::Result<SandboxOutput> {
    if output.timed_out || output.signal.is_some() { return Ok(output); }
    anyhow::ensure!(output.exit_code == Some(0), "SANDBOX_UNAVAILABLE: {}", output.output);
    let prefix = format!("\n{marker}:");
    let (body, trailer) = output.output.rsplit_once(&prefix)
        .ok_or_else(|| anyhow::anyhow!("SANDBOX_INVALID_RESULT"))?;
    let fields: Vec<_> = trailer.trim().split(':').collect();
    anyhow::ensure!(fields.len() == 3, "SANDBOX_INVALID_RESULT");
    let code: i32 = fields[0].parse()?;
    let signal: i32 = fields[1].parse()?;
    let timed_out: bool = fields[2].parse()?;
    output = SandboxOutput { output: body.into(), exit_code: Some(code),
        signal: if signal > 0 { Some(signal) } else { None }, timed_out };
    Ok(output)
}
impl SandboxExecutor for DockerTools {
    fn execute(&self, operation: &str, args: &Value) -> anyhow::Result<SandboxOutput> {
        let (cwd, script) = self.script(operation, args)?;
        let budget = timeout(args)?;
        let requested = args.get("timeout").and_then(Value::as_f64);
        let seconds = requested.filter(|n| n.is_finite() && *n > 0.0)
            .map_or(budget.as_secs_f64(), |n| n.min(budget.as_secs_f64()));
        let shell = text(args, "shell")?;
        anyhow::ensure!(["sh", "bash"].contains(&shell), "SandboxInvalidShell");
        let limit = output_limit(args)?;
        let mut nonce = [0u8; 16];
        getrandom::getrandom(&mut nonce).map_err(|e| anyhow::anyhow!("SandboxRandomFailed: {e}"))?;
        let marker = format!("polaris-{:x}", u128::from_le_bytes(nonce));
        // Merge command stderr inside the container; the supervisor trailer remains last.
        let script = format!("exec 2>&1\n{script}");
        let mut command = Command::new("docker");
        command.args(["exec", "-w", &cwd, &self.container, "sh", "-c", SUPERVISOR,
            "polaris-supervisor", shell, &script, &seconds.to_string(), &marker]);
        let output = sandbox_process::run(&mut command, budget, limit)?;
        decode(output, &marker)
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

#[cfg(all(test, unix))]
mod status_tests {
    use super::*;
    // Execute the identical container supervisor locally, never requiring Docker in unit tests.
    fn execute(command: &str, seconds: &str, shell: &str) -> anyhow::Result<SandboxOutput> {
        let mut process = Command::new("sh");
        process.args(["-c", SUPERVISOR, "test", shell, command, seconds, "test-marker"]);
        decode(sandbox_process::run(&mut process, Duration::from_secs(3), 4096)?, "test-marker")
    }
    #[test]
    fn preserves_failure_output_normal_exit_and_signal() -> anyhow::Result<()> {
        for shell in ["sh", "bash"] {
            for code in [0, 7, 124, 137, 143] {
                let result = execute(&format!("printf output; printf error >&2; exit {code}"), "2", shell)?;
                assert_eq!(result.exit_code, Some(code));
                assert_eq!(result.signal, None);
                assert!(!result.timed_out);
                assert!(result.output.contains("outputerror"));
            }
            let replaced = execute("exec sh -c 'exit 137'", "2", shell)?;
            assert_eq!(replaced.exit_code, Some(137));
            assert_eq!(replaced.signal, None);
            assert!(!replaced.timed_out);
            for (signal, code) in [(9, 137), (15, 143)] {
                let result = execute(&format!("kill -{signal} $$"), "2", shell)?;
                assert_eq!(result.exit_code, Some(code));
                assert_eq!(result.signal, Some(signal));
                assert!(!result.timed_out);
            }
        }
        Ok(())
    }
    #[test]
    fn timeout_is_distinct_from_failure_and_signal() -> anyhow::Result<()> {
        let result = execute("sleep 5", "0.05", "sh")?;
        assert!(result.timed_out);
        assert_eq!(result.exit_code, Some(137));
        assert_eq!(result.signal, Some(9));
        Ok(())
    }
    #[test]
    fn unresponsive_transport_has_bounded_wait() -> anyhow::Result<()> {
        let start = Instant::now();
        let result = sandbox_process::run(Command::new("sleep").arg("5"), Duration::from_millis(20), 1024)?;
        assert!(result.timed_out);
        assert!(start.elapsed() < Duration::from_millis(500));
        assert_eq!(result.exit_code, None);
        Ok(())
    }
}

use super::*;
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Owner {
    pub hostname: String, pub pid: u32, pub process_start: String, pub acquired_at: u64,
}
#[derive(Debug, Deserialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub(super) enum ProcessState { Alive { start: String }, Dead, Unknown }
pub(super) fn hostname() -> Result<String, String> {
    #[cfg(target_os = "linux")]
    let name = fs::read_to_string("/proc/sys/kernel/hostname").map_err(|_| "CredentialStoreHostnameFailed")?;
    #[cfg(not(target_os = "linux"))]
    let name = {
        let output = Command::new("hostname").output().map_err(|_| "CredentialStoreHostnameFailed")?;
        if !output.status.success() { return Err("CredentialStoreHostnameFailed".into()); }
        String::from_utf8(output.stdout).map_err(|_| "CredentialStoreHostnameFailed")?
    };
    Ok(name.trim().into())
}
pub(super) fn process_state(pid: u32) -> ProcessState {
    #[cfg(target_os = "linux")]
    {
        let stat = match fs::read_to_string(format!("/proc/{pid}/stat")) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return ProcessState::Dead,
            Err(_) => return ProcessState::Unknown,
        };
        let Some((_, rest)) = stat.rsplit_once(") ") else { return ProcessState::Unknown; };
        let fields: Vec<_> = rest.split_whitespace().collect();
        if matches!(fields.first(), Some(&"Z" | &"X")) { return ProcessState::Dead; }
        let (Some(start), Ok(boot)) = (fields.get(19), fs::read_to_string("/proc/sys/kernel/random/boot_id"))
            else { return ProcessState::Unknown; };
        ProcessState::Alive { start: format!("{}:{start}", boot.trim()) }
    }
    #[cfg(not(target_os = "linux"))]
    {
        #[cfg(windows)]
        let output = Command::new("powershell.exe").args(["-NoProfile", "-NonInteractive", "-Command",
            &format!("try {{ $p = Get-Process -Id {pid} -ErrorAction Stop; $p.StartTime.ToUniversalTime().Ticks }} catch {{ if ($_.FullyQualifiedErrorId -like 'NoProcessFoundForGivenId*') {{ 'dead' }} else {{ exit 2 }} }}")]).output();
        #[cfg(not(windows))]
        let output = Command::new("ps").args(["-p", &pid.to_string(), "-o", "lstart="]).env("LC_ALL", "C").output();
        match output {
            Ok(o) if o.status.success() => {
                let start = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if start.is_empty() || start == "dead" { ProcessState::Dead } else { ProcessState::Alive { start } }
            }
            #[cfg(not(windows))]
            Ok(o) if o.status.code() == Some(1) && o.stdout.is_empty() => ProcessState::Dead,
            _ => ProcessState::Unknown,
        }
    }
}
pub(super) fn owner() -> Result<Owner, String> {
    let pid = std::process::id();
    let ProcessState::Alive { start } = process_state(pid) else { return Err("CredentialStoreProcessIdentityFailed".into()); };
    Ok(Owner { hostname: hostname()?, pid, process_start: start, acquired_at: now_ms() })
}
pub(super) fn reclaimable(info: &Owner, host: &str, process: &ProcessState, now: u64, max: u64) -> bool {
    now.saturating_sub(info.acquired_at) > max || (info.hostname == host && match process {
        ProcessState::Dead => true,
        ProcessState::Alive { start } => start != &info.process_start,
        ProcessState::Unknown => false,
    })
}

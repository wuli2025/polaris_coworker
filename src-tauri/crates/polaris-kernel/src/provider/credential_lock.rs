//! Shared generation-directory algorithm with agentd/models/credential-lock.ts.
use super::*;
use super::lock_owner::{self, Owner, ProcessState};

pub(super) fn nonce() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).map_err(|_| "CredentialStoreRandomFailed")?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
// Only this unique generation can be detached. A delayed reaper/drop can never
// rename a replacement owner. Removing the outer directory is nonrecursive.
pub(super) fn retire(lock: &Path, token: &str) -> Result<bool, String> {
    let tomb = PathBuf::from(format!("{}.tomb.{}", lock.display(), nonce()?));
    match fs::rename(lock.join(token), &tomb) {
        Ok(()) => {},
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(_) => return Err("CredentialStoreReclaimFailed".into()),
    }
    let _ = fs::remove_dir(lock);
    fs::remove_dir_all(tomb).map_err(|_| "CredentialStoreReclaimFailed")?;
    Ok(true)
}
pub(super) fn reclaim(lock: &Path, max: u64) -> Result<bool, String> {
    let entries = match fs::read_dir(lock) { Ok(e) => e, Err(_) => return Ok(false) };
    let host = lock_owner::hostname()?;
    for entry in entries.flatten() {
        let token = entry.file_name().to_string_lossy().to_string();
        if !token.strip_prefix("owner-").is_some_and(|s| !s.is_empty()
            && s.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase() || c == '-')) { continue; }
        let info = fs::read(entry.path().join("owner.json")).ok()
            .and_then(|b| serde_json::from_slice::<Owner>(&b).ok());
        let Some(info) = info.filter(|o| o.pid > 0 && !o.hostname.is_empty() && !o.process_start.is_empty()
            && o.acquired_at <= 9007199254740991) else {
            let age = entry.metadata().ok().and_then(|m| m.modified().ok())
                .and_then(|t| SystemTime::now().duration_since(t).ok());
            if age.is_some_and(|age| age > Duration::from_millis(max)) { return retire(lock, &token); }
            continue;
        };
        let process = if info.hostname == host { lock_owner::process_state(info.pid) } else { ProcessState::Unknown };
        if lock_owner::reclaimable(&info, &host, &process, now_ms(), max) { return retire(lock, &token); }
    }
    let _ = fs::remove_dir(lock); // Empty retired/legacy container only.
    Ok(false)
}
pub struct FileLock { lock: PathBuf, token: String }
impl FileLock {
    pub fn acquire(path: &Path) -> Result<Self, String> {
        let settings = super::pi_config::load()?;
        Self::acquire_with(path, settings.lock_timeout_ms, settings.lock_poll_ms, settings.lock_max_hold_ms)
    }
    pub(super) fn acquire_with(path: &Path, timeout: u64, poll: u64, max: u64) -> Result<Self, String> {
        let lock = PathBuf::from(format!("{}.lock", path.display()));
        let token = format!("owner-{}", nonce()?);
        let candidate = PathBuf::from(format!("{}.candidate.{}", lock.display(), nonce()?));
        fs::create_dir_all(candidate.join(&token)).map_err(|_| "CredentialStoreLockFailed")?;
        let result = (|| {
            let mut info = lock_owner::owner()?;
            let deadline = Instant::now() + Duration::from_millis(timeout);
            loop {
                info.acquired_at = now_ms();
                let bytes = serde_json::to_vec(&info).map_err(|_| "CredentialStoreLockFailed")?;
                fs::write(candidate.join(&token).join("owner.json"), bytes).map_err(|_| "CredentialStoreLockFailed")?;
                match fs::rename(&candidate, &lock) {
                    Ok(()) => return Ok(Self { lock, token }),
                    Err(_) if lock.is_dir() => {
                        reclaim(&lock, max)?;
                        if Instant::now() >= deadline { return Err("CredentialStoreBusy".into()); }
                        thread::sleep(Duration::from_millis(poll));
                    }
                    Err(_) => return Err("CredentialStoreLockFailed".into()),
                }
            }
        })();
        let _ = fs::remove_dir_all(candidate);
        result
    }
    pub fn assert_owned(&self) -> Result<(), String> {
        if self.lock.join(&self.token).join("owner.json").is_file() { Ok(()) }
        else { Err("CredentialStoreLeaseExpired".into()) }
    }
}
impl Drop for FileLock { fn drop(&mut self) { let _ = retire(&self.lock, &self.token); } }
#[cfg(test)]
#[path = "credential_lock_tests.rs"]
mod tests;

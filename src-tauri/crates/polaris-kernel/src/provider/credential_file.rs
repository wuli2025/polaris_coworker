//! Shared with agentd/models/file-credentials.ts. mkdir lock covers the entire
//! read/refresh/write transaction. Never steal a lock from a slow refresh.
use super::*;

pub fn provider_store_path() -> Option<PathBuf> { store::shared_path() }

pub struct FileLock(PathBuf);
impl FileLock {
    pub fn acquire(path: &Path) -> Result<Self, String> {
        let settings = super::pi_config::load()?;
        let lock = PathBuf::from(format!("{}.lock", path.display()));
        let deadline = Instant::now() + Duration::from_millis(settings.lock_timeout_ms);
        loop {
            match fs::create_dir(&lock) {
                Ok(()) => return Ok(Self(lock)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    if Instant::now() >= deadline { return Err("CredentialStoreBusy".into()); }
                    thread::sleep(Duration::from_millis(settings.lock_poll_ms));
                }
                Err(_) => return Err("CredentialStoreLockFailed".into()),
            }
        }
    }
}
impl Drop for FileLock { fn drop(&mut self) { let _ = fs::remove_dir(&self.0); } }

pub fn read_root(path: &Path) -> Result<Value, String> {
    match fs::read(path) {
        Ok(bytes) => {
            let root: Value = serde_json::from_slice(&bytes).map_err(|_| "CredentialStoreInvalid")?;
            if !root.is_object() { return Err("CredentialStoreInvalid".into()); }
            Ok(root)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(_) => Err("CredentialStoreReadFailed".into()),
    }
}

pub fn read(id: &str) -> Result<Option<Value>, String> {
    let Some(path) = provider_store_path() else { return Ok(None); };
    Ok(read_root(&path)?.get("credentials").and_then(|c| c.get(id)).cloned())
}

pub fn write(id: &str, credential: Option<Value>) -> Result<(), String> {
    let path = provider_store_path().ok_or("ProviderStoreNotInitialized")?;
    let _lock = FileLock::acquire(&path)?;
    let mut root = read_root(&path)?;
    let entries = root.as_object_mut().ok_or("CredentialStoreInvalid")?
        .entry("credentials").or_insert_with(|| json!({}));
    let entries = entries.as_object_mut().ok_or("CredentialStoreInvalid")?;
    if let Some(credential) = credential { entries.insert(id.into(), credential); }
    else { entries.remove(id); }
    let text = serde_json::to_string_pretty(&root).map_err(|_| "CredentialStoreInvalid")?;
    atomic_write(&path, &text).map_err(|_| "CredentialStoreWriteFailed".into())
}

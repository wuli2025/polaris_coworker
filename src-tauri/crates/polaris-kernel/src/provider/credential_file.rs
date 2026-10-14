//! Shared with agentd/models/file-credentials.ts. Generation lock covers the entire
//! read/refresh/write transaction; reclaim only a dead owner or expired lease.
use super::*;

pub fn provider_store_path() -> Option<PathBuf> { store::shared_path() }

pub use super::credential_lock::FileLock;

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
    _lock.assert_owned()?;
    atomic_write(&path, &text).map_err(|_| "CredentialStoreWriteFailed".into())
}

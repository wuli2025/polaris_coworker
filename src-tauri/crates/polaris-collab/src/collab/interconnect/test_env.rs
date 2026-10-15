//! Serial, restoring fixture for tests that need the existing process-global DB API.
use super::super::db;

pub(crate) struct TestEnv {
    _guard: std::sync::MutexGuard<'static, ()>,
    previous: Vec<(&'static str, Option<std::ffi::OsString>)>,
    pub directory: std::path::PathBuf,
}

impl TestEnv {
    pub fn new(tag: &str) -> Result<Self, String> {
        let guard = db::TEST_LOCK.lock().map_err(|e| e.to_string())?;
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?.as_nanos();
        let directory = std::env::temp_dir().join(format!("p6a-{}-{stamp}-{tag}", std::process::id()));
        std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        let mut env = Self { _guard: guard, previous: vec![], directory };
        for key in [
            "POLARIS_ACCOUNT_AUTHORITY", "POLARIS_ACCOUNT_AUTHORITY_URL",
            "POLARIS_ACCOUNT_AUTHORITY_PUBLIC_KEY_FINGERPRINTS", "POLARIS_RELAYS",
            "POLARIS_RELAY_FALLBACK", "POLARIS_INTERCONNECT_CONFIG", "POLARIS_COLLAB_DB",
            "POLARIS_ACCOUNT_KEY",
        ] {
            env.previous.push((key, std::env::var_os(key)));
            std::env::remove_var(key);
        }
        std::fs::write(env.directory.join("config.json"), super::DEFAULTS).map_err(|e| e.to_string())?;
        std::env::set_var("POLARIS_INTERCONNECT_CONFIG", env.directory.join("config.json"));
        std::env::set_var("POLARIS_COLLAB_DB", env.directory.join("collab.db"));
        std::env::set_var("POLARIS_ACCOUNT_KEY", env.directory.join("authority.key"));
        Ok(env)
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        for (key, value) in &self.previous {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

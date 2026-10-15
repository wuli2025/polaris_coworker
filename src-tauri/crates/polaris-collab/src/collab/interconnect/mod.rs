//! Bundled defaults < user JSON < environment. Null authority means local mode.
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

pub mod gate;
pub mod notifications;
pub mod risk;
pub mod sync;
pub mod devices;
pub mod revocations;
pub mod binding;
pub mod login;
pub mod risk_inventory;

pub const DEFAULTS: &str = include_str!("defaults.json");
pub const SCHEMA: &str = include_str!("schema.json");

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub authority: Authority,
    pub relay: Relay,
    pub security: Security,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Authority {
    pub url: Option<String>,
    pub public_key_fingerprints: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Relay {
    pub urls: Vec<String>,
    pub fallback: bool,
}


#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Security {
    pub revocation_poll_secs: u64,
    pub revocation_max_age_secs: u64,
    pub new_device_window_hours: u64,
    pub approval_timeout_secs: u64,
}

fn merge(base: &mut serde_json::Value, overlay: serde_json::Value) {
    match (base, overlay) {
        (serde_json::Value::Object(base), serde_json::Value::Object(overlay)) => {
            for (key, value) in overlay {
                merge(base.entry(key).or_insert(serde_json::Value::Null), value);
            }
        }
        (base, value) => *base = value,
    }
}

/// Pure resolver, also used by tests without changing process-wide environment.
pub fn resolve(
    user_json: Option<&str>,
    env: impl Fn(&str) -> Option<String>,
) -> Result<Config, String> {
    let mut value: serde_json::Value = serde_json::from_str(DEFAULTS).map_err(|e| e.to_string())?;
    if let Some(raw) = user_json {
        let user: serde_json::Value = serde_json::from_str(raw)
            .map_err(|e| format!("互联用户配置不是合法 JSON: {e}"))?;
        if !user.is_object() {
            return Err("互联配置必须是 JSON 对象".into());
        }
        merge(&mut value, user);
    }
    // Deserialize before applying env to avoid panics from indexing malformed user JSON.
    let mut config: Config = serde_json::from_value(value)
        .map_err(|e| format!("互联配置类型错误: {e}"))?;
    if let Some(url) = env("POLARIS_ACCOUNT_AUTHORITY_URL") {
        config.authority.url = Some(url);
    }
    if let Some(raw) = env("POLARIS_ACCOUNT_AUTHORITY_PUBLIC_KEY_FINGERPRINTS") {
        config.authority.public_key_fingerprints = csv(&raw);
    }
    if let Some(raw) = env("POLARIS_RELAYS") {
        config.relay.urls = csv(&raw);
    }
    if let Some(raw) = env("POLARIS_RELAY_FALLBACK") {
        config.relay.fallback = match raw.as_str() {
            "true" | "1" => true,
            "false" | "0" => false,
            _ => return Err("POLARIS_RELAY_FALLBACK 须为 true/false/1/0".into()),
        };
    }
    config.authority.url = config.authority.url
        .map(|v| v.trim().trim_end_matches('/').to_string())
        .filter(|v| !v.is_empty());
    if let Some(url) = &config.authority.url {
        validate_url(url)?;
    }
    for fp in &mut config.authority.public_key_fingerprints {
        if fp.len() != 64 || !fp.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err("权威预置指纹必须为完整 SHA-256（64 位十六进制），不能使用短 kid".into());
        }
        fp.make_ascii_lowercase();
    }
    for url in &config.relay.urls {
        validate_url(url)?;
    }
    for (name, value, max) in [
        ("revocationPollSecs", config.security.revocation_poll_secs, 3600),
        ("revocationMaxAgeSecs", config.security.revocation_max_age_secs, 86400),
        ("newDeviceWindowHours", config.security.new_device_window_hours, 720),
        ("approvalTimeoutSecs", config.security.approval_timeout_secs, 3600),
    ] {
        if value == 0 || value > max { return Err(format!("security.{name} 须为 1..={max} 的整数")); }
    }
    Ok(config)
}

fn csv(raw: &str) -> Vec<String> {
    raw.split(',').map(str::trim).filter(|v| !v.is_empty()).map(String::from).collect()
}

pub fn validate_url(raw: &str) -> Result<(), String> {
    ureq::get(raw).request_url().map_err(|e| format!("互联 URL 非法: {e}"))?;
    let rest = raw.strip_prefix("https://").or_else(|| raw.strip_prefix("http://"))
        .ok_or_else(|| format!("互联 URL 只支持 http/https: {raw}"))?;
    let host = rest.split('/').next().ok_or("互联 URL 缺主机名")?;
    if host.is_empty() || raw.chars().any(char::is_whitespace)
        || raw.contains(['@', '#', '?', '\\'])
    {
        return Err(format!("互联 URL 非法（不允许凭据、查询或片段）: {raw}"));
    }
    Ok(())
}

fn user_path() -> Option<PathBuf> {
    match std::env::var("POLARIS_INTERCONNECT_CONFIG") {
        Ok(path) => Some(PathBuf::from(path)),
        Err(_) => directories::UserDirs::new()
            .map(|u| u.home_dir().join("Polaris/data/interconnect.json")),
    }
}

pub fn load() -> Result<Config, String> {
    let raw = match user_path() {
        Some(path) => match std::fs::read_to_string(&path) {
            Ok(raw) => Some(raw),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound
                && std::env::var_os("POLARIS_INTERCONNECT_CONFIG").is_none() => None,
            Err(e) => return Err(format!("读取互联配置 {} 失败: {e}", path.display())),
        },
        None => None,
    };
    resolve(raw.as_deref(), |key| std::env::var(key).ok())
}

/// Authority responses advertise only URLs; fallback remains a client preference.
pub fn advertised_relays() -> Result<Vec<String>, String> {
    Ok(load()?.relay.urls)
}

/// Shared by password login, OTP login, enrollment and heartbeat clients.
/// Old authorities without this field preserve the existing local relay policy.
pub fn apply_received_relays(response: &serde_json::Value) -> Result<(), String> {
    let Some(raw) = response.get("relays") else { return Ok(()); };
    let urls: Vec<String> = serde_json::from_value(raw.clone())
        .map_err(|e| format!("权威 relay 列表格式错误: {e}"))?;
    for url in &urls {
        validate_url(url)?;
    }
    #[cfg(feature = "collab-net")]
    super::tunnel::apply_authority_relays(urls)?;
    Ok(())
}

pub fn fingerprint(public_key_b64: &str) -> Result<String, String> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(public_key_b64).map_err(|e| format!("权威公钥不是 base64url: {e}"))?;
    let bytes: [u8; 32] = bytes.try_into().map_err(|_| "权威公钥长度须为 32 字节")?;
    ed25519_dalek::VerifyingKey::from_bytes(&bytes)
        .map_err(|e| format!("权威公钥无效: {e}"))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

impl Config {
    /// No configured fingerprints means no first-contact trust, even for an explicit URL.
    pub fn verify_key(&self, public_key_b64: &str) -> Result<(), String> {
        if self.authority.public_key_fingerprints.is_empty() {
            return Err("未配置权威公钥指纹，拒绝首次信任；请配置 authority.publicKeyFingerprints".into());
        }
        let actual = fingerprint(public_key_b64)?;
        if !self.authority.public_key_fingerprints.contains(&actual) {
            return Err(format!("权威公钥指纹不符，拒绝连接：实际 SHA-256 {actual}"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) mod test_env;

#[cfg(test)]
mod security_tests;

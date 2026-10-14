use super::*;
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PiSettings {
    pub lock_timeout_ms: u64,
    pub lock_poll_ms: u64,
    pub lock_max_hold_ms: u64,
    pub context_window: u64,
    pub max_tokens: u64,
    pub reasoning: bool,
    pub input: Vec<String>,
    pub cost: HashMap<String, f64>,
    pub aliases: HashMap<String, String>,
}
pub fn load() -> Result<PiSettings, String> {
    let mut value: Value = serde_json::from_str(include_str!("pi-default.json"))
        .map_err(|_| "ProviderConfigInvalid")?;
    if let Ok(path) = std::env::var("POLARIS_PROVIDER_CONFIG") {
        let patch: Value = serde_json::from_slice(&fs::read(path).map_err(|_| "ProviderConfigReadFailed")?)
            .map_err(|_| "ProviderConfigInvalid")?;
        let patch = patch.as_object().ok_or("ProviderConfigInvalid")?;
        let target = value.as_object_mut().ok_or("ProviderConfigInvalid")?;
        target.extend(patch.clone());
    }
    let settings: PiSettings = serde_json::from_value(value).map_err(|_| "ProviderConfigInvalid")?;
    if [settings.lock_max_hold_ms, settings.lock_timeout_ms, settings.lock_poll_ms]
        .iter().any(|v| *v == 0 || *v > 9007199254740991) || settings.context_window == 0
        || settings.max_tokens == 0 || settings.max_tokens > settings.context_window
        || settings.input.is_empty() || settings.input.iter().any(|s| s != "text" && s != "image")
        || ["input", "output", "cacheRead", "cacheWrite"].iter()
            .any(|k| settings.cost.get(*k).is_none_or(|v| !v.is_finite() || *v < 0.0)) {
        return Err("ProviderConfigInvalid".into());
    }
    Ok(settings)
}

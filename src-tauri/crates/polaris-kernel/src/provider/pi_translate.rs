//! Polaris -> Pi config. Credentials never enter the returned configuration.
use super::*;
use super::pi_config::PiSettings;
#[path = "pi_model.rs"]
mod pi_model;

/// Legacy protocol names describe wire formats, not model vendors. Pi 1.0.4
/// API names are verified against pi-ai/dist/api/*.d.ts and models.md.
pub const PROTOCOL_APIS: &[(&str, &str)] = &[
    ("", "anthropic-messages"), ("anthropic", "anthropic-messages"),
    ("openai", "openai-completions"), ("responses", "openai-responses"),
    ("chatgpt", "openai-codex-responses"), ("google", "google-generative-ai"),
];
/// Preserve the SDK's native subscription auth and request adapters.
pub fn credential_id(id: &str) -> &str {
    match id { "claude-official" => "anthropic", "codex" => "openai-codex", _ => id }
}
pub fn polaris_id(id: &str) -> &str {
    match id { "anthropic" => "claude-official", "openai-codex" => "codex", _ => id }
}
const MODEL_VARIABLES: &[&str] = &[
    "ANTHROPIC_MODEL", "ANTHROPIC_DEFAULT_OPUS_MODEL", "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL", "ANTHROPIC_SMALL_FAST_MODEL", "OPENAI_MODEL",
];
fn text<'a>(value: &'a Value, key: &str) -> &'a str { value.get(key).and_then(Value::as_str).unwrap_or("") }
fn model(value: &Value, defaults: &PiSettings) -> Result<Value, String> {
    let id = value.as_str().unwrap_or_else(|| text(value, "id"));
    if id.trim().is_empty() { return Err("ProviderModelInvalid [paths: /settingsConfig/models]".into()); }
    let mut result = json!({"id":id,"name":id,"reasoning":defaults.reasoning,
        "input":defaults.input,"cost":defaults.cost,"contextWindow":defaults.context_window,"maxTokens":defaults.max_tokens});
    for key in ["name", "reasoning", "input", "cost", "contextWindow", "maxTokens"] {
        if let Some(v) = value.get(key) { result[key] = v.clone(); }
    }
    Ok(result)
}
static CATALOG: Lazy<parking_lot::RwLock<Option<Vec<Value>>>> = Lazy::new(|| parking_lot::RwLock::new(None));

/// Only successful SDK catalog reads refresh this process-local, non-secret cache.
pub fn cache_catalog(catalog: &Value) {
    if let Some(models) = catalog.get("models").and_then(Value::as_array) {
        *CATALOG.write() = Some(models.clone());
    }
}
pub fn validate_local(view: &ProviderView) -> Result<(), String> {
    let catalog = CATALOG.read().clone();
    let defaults = super::pi_config::load()?;
    translate_inner(view, catalog.as_deref().unwrap_or_default(), &defaults, catalog.is_some()).map(|_| ())
}
pub fn translate(view: &ProviderView, catalog: &[Value], defaults: &PiSettings) -> Result<Value, String> {
    translate_inner(view, catalog, defaults, true)
}
fn translate_inner(view: &ProviderView, catalog: &[Value], defaults: &PiSettings, complete: bool) -> Result<Value, String> {
    let native = matches!(view.id.as_str(), "codex" | "claude-official");
    let api = PROTOCOL_APIS.iter().find(|(p, _)| *p == view.protocol)
        .map(|(_, a)| *a).ok_or("ProviderProtocolUnsupported [paths: /protocol]")?;
    let builtin = defaults.aliases.get(&view.id).map(String::as_str).unwrap_or(&view.id);
    let matching: Vec<&Value> = catalog.iter().filter(|m| text(m, "provider") == builtin).collect();
    let supplied = view.settings_config.get("models");
    let sources: Vec<Value> = if let Some(supplied) = supplied {
        supplied.as_array().filter(|a| !a.is_empty()).ok_or("ProviderModelsInvalid [paths: /settingsConfig/models]")?.clone()
    } else {
        let env: Vec<Value> = MODEL_VARIABLES.iter().filter_map(|key| {
            view.settings_config.get("env")?.get(*key)?.as_str().filter(|s| !s.trim().is_empty()).map(|s| json!(s))
        }).collect();
        if env.is_empty() { matching.iter().map(|m| (*m).clone()).collect() } else { env }
    };
    if complete && sources.is_empty() { return Err("ProviderModelsMissing [paths: /settingsConfig/models]".into()); }
    let mut seen = HashSet::new();
    let mut models = Vec::new();
    for (index, source) in sources.into_iter().enumerate() {
        let id = source.as_str().map_or_else(|| text(&source, "id"), |id| id);
        let known = matching.iter().find(|m| text(m, "id") == id);
        let mut m = model(known.copied().unwrap_or(&source), defaults)?;
        if source.is_object() {
            for key in ["name", "reasoning", "input", "cost", "contextWindow", "maxTokens"] {
                if let Some(value) = source.get(key) { m[key] = value.clone(); }
            }
        }
        pi_model::validate(&m, index)?;
        if seen.insert(text(&m, "id").to_string()) { models.push(m); }
    }
    if native {
        return Ok(json!({"id":credential_id(&view.id),"models":models}));
    }
    let base = if view.base_url.is_empty() {
        matching.first().map(|m| text(m, "baseUrl")).unwrap_or("")
    } else { &view.base_url };
    let authority = base.split("://").nth(1).unwrap_or("").split('/').next().unwrap_or("");
    if (!base.is_empty() || complete) && (!(base.starts_with("https://") || base.starts_with("http://")) || base.contains("${")
        || authority.is_empty() || authority.contains('@') || base.contains('?') || base.contains('#')) {
        return Err("ProviderBaseUrlInvalid [paths: /settingsConfig/env/ANTHROPIC_BASE_URL]".into());
    }
    let bearer = api == "anthropic-messages" && view.token_field == "ANTHROPIC_AUTH_TOKEN"
        && view.id != "claude-official";
    Ok(json!({"id":credential_id(&view.id),"baseUrl":base,"api":api,"models":models,"authHeader":bearer}))
}

pub fn config(id: &str, catalog: &Value) -> Result<Value, String> {
    let defaults = super::pi_config::load()?;
    let catalog = catalog.get("models").and_then(Value::as_array).ok_or("AgentdInvalidModels")?;
    let views = store::pi_views();
    let selected = views.iter().find(|v| v.id == id).ok_or("ProviderNotFound")?;
    let selected = translate(selected, catalog, &defaults)?;
    let default_model = json!({"provider":selected["id"],"id":selected["models"][0]["id"]});
    let mut providers = vec![selected];
    for view in views.iter().filter(|v| v.id != id) {
        // Unconfigured presets stay visible in the dock, but cannot be used until
        // their endpoint/model metadata is supplied. Selecting one fails above.
        if let Ok(provider) = translate(view, catalog, &defaults) { providers.push(provider); }
    }
    Ok(json!({"providers":providers,"defaultModel":default_model}))
}

pub fn is_pi() -> bool {
    crate::chat::backend::config::Config::load().map(|c| c.agent.backend != "claude-cli").unwrap_or(true)
}

#[cfg(test)]
#[path = "pi_translate_tests.rs"]
mod tests;

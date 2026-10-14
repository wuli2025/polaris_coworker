//! Existing shell commands delegate Pi login to SDK OAuth, with no token IPC.
use super::*;
fn call(provider: &str, operation: &str, input: Option<String>) -> Result<Value, String> {
    crate::chat::backend::selected().map_err(|_| "AgentBackendUnavailable")?
        .auth(operation, provider, input).map_err(|e| e.to_string())
}
fn string(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or_default().into()
}
pub fn claude_status() -> Result<ClaudeAuthStatus, String> {
    Ok(ClaudeAuthStatus { logged_in: credential_file::read("anthropic")?
        .is_some_and(|c| c["type"] == "oauth" && c["access"].as_str().is_some_and(|s| !s.is_empty())),
        cred_path: path() })
}
fn path() -> String {
    credential_file::provider_store_path().map(|p| p.to_string_lossy().into()).unwrap_or_default()
}
pub fn codex_status() -> Result<CodexStatus, String> {
    Ok(CodexStatus { installed: true, logged_in: credential_file::read("openai-codex")?
        .is_some_and(|c| c["type"] == "oauth" && c["access"].as_str().is_some_and(|s| !s.is_empty())), auth_path: path() })
}
pub fn claude_start() -> Result<ClaudeLoginStart, String> {
    let result = call("anthropic", "start", None)?;
    if result["status"] == "failed" { return Err("OAuthLoginFailed".into()); }
    let url = string(&result, "url");
    let _ = codex_open_browser(&url);
    Ok(ClaudeLoginStart { mode: "manual".into(), authorize_url: url, verifier: String::new(), state: String::new() })
}
pub fn claude_finish(input: String) -> Result<ClaudeAuthStatus, String> {
    let result = call("anthropic", "respond", Some(input))?;
    if result["status"] != "ok" { return Err("OAuthLoginFailed".into()); }
    claude_status()
}
pub fn codex_start(device: Option<bool>) -> Result<CodexDeviceLogin, String> {
    let device = device.unwrap_or(true);
    let result = call("openai-codex", "start", Some(if device { "device" } else { "browser" }.into()))?;
    if result["status"] == "failed" { return Err("OAuthLoginFailed".into()); }
    let url = string(&result, "url");
    let _ = codex_open_browser(&url);
    Ok(CodexDeviceLogin { mode: if device { "device" } else { "auto" }.into(),
        device_code: "pi".into(), user_code: string(&result, "userCode"), verification_uri: url,
        interval: result["interval"].as_u64().unwrap_or(5), expires_in: result["expiresIn"].as_u64().unwrap_or(600) })
}
pub fn poll(provider: &str) -> Result<LoginPollResult, String> {
    let result = call(provider, "poll", None)?;
    Ok(LoginPollResult { status: string(&result, "status"),
        message: if result["status"] == "failed" { "OAuthLoginFailed".into() } else { String::new() } })
}
pub fn cancel(provider: &str) -> Result<(), String> { call(provider, "cancel", None).map(|_| ()) }

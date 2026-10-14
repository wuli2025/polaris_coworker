//! Map Polaris keys to daemon UUIDs; never pass a conversation ID as a new UUID.
use super::{client::Client, protocol::{Command, Open, Session, Configure, SessionOptions}};
use super::super::TurnRequest;
use serde::{Serialize, Deserialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
#[derive(Clone, Serialize, Deserialize)]
pub struct SessionInfo { pub id: String, pub cwd: PathBuf, pub model: Value, pub tools: Vec<String> }
fn path(client: &Client, key: &str) -> PathBuf {
    client.config.session_map.join(format!("{:x}.json", Sha256::digest(key.as_bytes())))
}
pub fn open(client: &Client, input: &TurnRequest) -> anyhow::Result<SessionInfo> {
    let options = super::input::options(client, input)?;
    let file = path(client, &input.session_id);
    let previous: Option<SessionInfo> = match std::fs::read(&file) {
        Ok(bytes) => Some(serde_json::from_slice(&bytes)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    let cwd = std::fs::canonicalize(&input.cwd)?;
    let id = previous.as_ref().map(|s| s.id.clone());
    if let Some(previous) = &previous {
        if previous.cwd != cwd { let _ = client.call(Command::Close(Session { session_id: previous.id.clone() })); }
    }
    let response = client.call(Command::Open(Open { session_id: id, cwd: Some(cwd.to_string_lossy().into()), options: options.clone() }));
    let response = match response {
        Ok(v) => v,
        Err(e) if e.to_string().starts_with("SESSION_ALREADY_OPEN:") => {
            let previous = previous.as_ref().ok_or_else(|| anyhow::anyhow!("AgentdSessionMappingMissing"))?;
            client.call(Command::Configure(Configure { session_id: previous.id.clone(), options: options.clone() }))?
        }
        Err(e) if e.to_string().starts_with("SESSION_NOT_FOUND:") => {
            client.call(Command::Open(Open { session_id: None, cwd: Some(cwd.to_string_lossy().into()), options: options.clone() }))?
        }
        Err(e) => return Err(e),
    };
    anyhow::ensure!(response.get("reopenRequired") != Some(&Value::Bool(true)), "AgentdSessionReopenRequired");
    let info = SessionInfo {
        id: response.get("sessionId").and_then(Value::as_str).ok_or_else(|| anyhow::anyhow!("AgentdMissingSessionId"))?.into(),
        cwd, model: response.get("model").cloned().ok_or_else(|| anyhow::anyhow!("AgentdMissingModel"))?,
        tools: serde_json::from_value(response.get("tools").cloned().ok_or_else(|| anyhow::anyhow!("AgentdMissingTools"))?)?,
    };
    std::fs::create_dir_all(&client.config.session_map)?;
    let temporary = file.with_extension("tmp");
    std::fs::write(&temporary, serde_json::to_vec(&info)?)?;
    std::fs::rename(temporary, file)?;
    Ok(info)
}
pub fn close(client: &Client, key: &str) -> anyhow::Result<()> {
    let file = path(client, key);
    if !file.exists() { return Ok(()); }
    let info: SessionInfo = serde_json::from_slice(&std::fs::read(&file)?)?;
    match client.call(Command::Close(Session { session_id: info.id })) {
        Err(e) if !e.to_string().starts_with("SESSION_NOT_FOUND:") => Err(e),
        _ => Ok(()),
    }
}

/// The SDK is authoritative for busy detection; an active turn rejects configuration.
pub fn configure(client: &Client, key: &str, options: SessionOptions) -> anyhow::Result<Value> {
    let info: SessionInfo = serde_json::from_slice(&std::fs::read(path(client, key))?)?;
    client.call(Command::Configure(Configure { session_id: info.id, options }))
}

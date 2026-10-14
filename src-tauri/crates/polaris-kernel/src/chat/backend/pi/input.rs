//! Host input conversion; no provider aliases or attachment paths are guessed.
use super::{client::Client, protocol::{Command, Empty, Model, SessionOptions}};
use super::super::{TurnRequest, PermissionMode};

pub fn options(client: &Client, input: &TurnRequest) -> anyhow::Result<SessionOptions> {
    let model = match &input.model {
        None => None,
        Some(serde_json::Value::String(id)) => {
            // Existing POLARIS_*_MODEL values contain only an ID. Resolve it only
            // when the daemon catalog identifies exactly one provider.
            let catalog = client.call(Command::Models(Empty {}))?;
            let models: Vec<Model> = serde_json::from_value(catalog.get("models").cloned()
                .ok_or_else(|| anyhow::anyhow!("AgentdMissingModels"))?
                .as_array().ok_or_else(|| anyhow::anyhow!("AgentdInvalidModels"))?
                .iter().map(|m| serde_json::json!({"provider":m["provider"], "id":m["id"]})).collect::<Vec<_>>().into())?;
            let mut matches = models.into_iter().filter(|m| &m.id == id);
            let model = matches.next().ok_or_else(|| anyhow::anyhow!("AgentdModelNotFound: {id}"))?;
            anyhow::ensure!(matches.next().is_none(), "AgentdModelAmbiguous: specify provider and id");
            Some(model)
        }
        Some(value) => Some(serde_json::from_value::<Model>(value.clone())?),
    };
    if let Some(model) = &model {
        anyhow::ensure!(!model.provider.is_empty() && !model.id.is_empty(), "AgentdInvalidModel");
    }
    Ok(SessionOptions { model, thinking_level: input.thinking_level.clone(),
        permission_mode: Some(match input.permission {
            PermissionMode::Deny => super::protocol::PermissionMode::Deny,
            PermissionMode::Manual => super::protocol::PermissionMode::Manual,
            PermissionMode::AutoCurrent => super::protocol::PermissionMode::AutoCurrent,
            PermissionMode::AutoAll => super::protocol::PermissionMode::AutoAll,
        }), sandbox: None, expert_prompt: None,
        tool_profile: Some(match input.permission { PermissionMode::Deny => "readonly", _ => "workspace" }.into()) })
}

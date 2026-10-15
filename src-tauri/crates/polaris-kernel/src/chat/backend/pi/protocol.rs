//! Exact wire names from agentd/src/protocol/{schema,events}.ts.
use serde::{Deserialize, Serialize};
use serde_json::Value;
/// Mirror of agentd/src/protocol/schema.ts; fingerprint test gates changes.
pub const PROTOCOL_VERSION: u64 = 6;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(untagged)]
pub enum Id { Text(String), Number(i64) }
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request { pub id: Id, #[serde(flatten)] pub command: Command }
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum Command {
    #[serde(rename = "provider.auth")] Auth(Auth),
    #[serde(rename = "permission.respond")] PermissionRespond(PermissionResponse),
    #[serde(rename = "sandbox.respond")] SandboxRespond(SandboxResponse),
    #[serde(rename = "ping")] Ping(Empty),
    #[serde(rename = "models.list")] Models(Empty),
    #[serde(rename = "models.catalog")] Catalog(Empty),
    #[serde(rename = "session.open")] Open(Open),
    #[serde(rename = "session.configure")] Configure(Configure),
    #[serde(rename = "prompt")] Prompt(Prompt),
    #[serde(rename = "steer")] Steer(Text),
    #[serde(rename = "followUp")] FollowUp(Text),
    #[serde(rename = "abort")] Abort(Session),
    #[serde(rename = "close")] Close(Session),
    #[serde(rename = "config.apply")] Apply(Apply),
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PermissionMode { Deny, Manual, AutoCurrent, AutoAll }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Decision { Allow, Deny, AllowAlways }
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PermissionResponse { pub session_id: String, pub call_id: String, pub decision: Decision }
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SandboxResponse {
    pub session_id: String, pub call_id: String, pub output: String,
    #[serde(skip_serializing_if = "Option::is_none")] pub error: Option<String>,
    pub exit_code: Option<i32>, pub signal: Option<i32>, pub timed_out: bool,
}
#[derive(Debug, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Empty {}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Open {
    #[serde(skip_serializing_if = "Option::is_none")] pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub cwd: Option<String>,
    #[serde(flatten)] pub options: SessionOptions,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Model { pub provider: String, pub id: String }
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ThinkingLevel { Off, Minimal, Low, Medium, High, Xhigh }
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionOptions {
    #[serde(skip_serializing_if = "Option::is_none")] pub model: Option<Model>,
    #[serde(skip_serializing_if = "Option::is_none")] pub thinking_level: Option<ThinkingLevel>,
    #[serde(skip_serializing_if = "Option::is_none")] pub tool_profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub permission_mode: Option<PermissionMode>,
    #[serde(skip_serializing_if = "Option::is_none")] pub sandbox: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")] pub expert_prompt: Option<String>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Configure { pub session_id: String, #[serde(flatten)] pub options: SessionOptions }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum Attachment {
    #[serde(rename_all = "camelCase")]
    Image { mime_type: String, data: String },
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Prompt {
    pub session_id: String, pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")] pub attachments: Vec<Attachment>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ping {
    pub status: String, pub sessions: u64, pub version: String,
    pub pi_version: String, pub protocol_version: u64, pub usage_persist_failures: u64,
    pub experts: Option<super::experts::ExpertConfig>,
}
pub fn check_ping(value: Value) -> anyhow::Result<Ping> {
    anyhow::ensure!(value.get("protocolVersion").and_then(Value::as_u64) == Some(PROTOCOL_VERSION),
        "AgentdProtocolMismatch: expected protocolVersion={PROTOCOL_VERSION}");
    let ping: Ping = serde_json::from_value(value)?;
    anyhow::ensure!(ping.status == "ok" && !ping.version.is_empty() && !ping.pi_version.is_empty(), "AgentdPingFailed");
    Ok(ping)
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Text { pub session_id: String, pub text: String }
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Session { pub session_id: String }
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Apply {
    #[serde(skip_serializing_if = "Option::is_none")] pub config: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")] pub credentials: Option<Value>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct WireError { pub code: String, pub message: String, pub paths: Option<Vec<String>> }
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum Frame {
    #[serde(rename = "response")]
    Response { id: Option<Id>, ok: bool, result: Option<Value>, error: Option<WireError> },
    #[serde(rename = "event", rename_all = "camelCase")]
    Event { session_id: String, turn_id: String, #[serde(flatten)] event: Event },
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionResolution { Responded, Expired, Cancelled }
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "event")]
pub enum Event {
    #[serde(rename = "permission.request", rename_all = "camelCase")]
    PermissionRequest { target_session_id: String, call_id: String, tool: String, summary: String, expires_at: u64, parent_call_id: Option<String> },
    #[serde(rename = "permission.resolved", rename_all = "camelCase")]
    PermissionResolved { target_session_id: String, call_id: String, reason: PermissionResolution },
    #[serde(rename = "sandbox.request", rename_all = "camelCase")]
    SandboxRequest { target_session_id: String, call_id: String, operation: String, args: Value },
    #[serde(rename = "turn.started")] Started,
    #[serde(rename = "text.delta")] Text { text: String },
    #[serde(rename = "reasoning.delta")] Reasoning { text: String },
    #[serde(rename = "tool.started", rename_all = "camelCase")]
    ToolStarted { call_id: String, tool: String, args: Value, parent_call_id: Option<String> },
    #[serde(rename = "tool.completed", rename_all = "camelCase")]
    ToolCompleted { call_id: String, tool: String, result: Value, is_error: bool, parent_call_id: Option<String> },
    #[serde(rename = "usage")] Usage { usage: Usage },
    #[serde(rename = "turn.completed")] Completed { status: TurnStatus },
    #[serde(rename = "error")] Error { code: String, message: String },
}
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TurnStatus { Completed, Aborted, Error, Timeout }
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage { pub input: f64, pub output: f64, pub cache_read: f64, pub cache_write: f64, pub total_tokens: f64, pub cost: Cost }
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cost { pub input: f64, pub output: f64, pub cache_read: f64, pub cache_write: f64, pub total: f64 }
impl Frame {
    pub fn parse(bytes: &[u8]) -> anyhow::Result<Self> {
        let raw: Value = serde_json::from_slice(bytes)?;
        let mut frame: Self = serde_json::from_value(raw.clone())?;
        if let Self::Response { result, .. } = &mut frame {
            if raw.get("result") == Some(&Value::Null) { *result = Some(Value::Null); }
        }
        if let Self::Response { id, ok, result, error } = &frame {
            anyhow::ensure!(if *ok { id.is_some() && result.is_some() && error.is_none() }
                else { error.is_some() && result.is_none() }, "InvalidProtocolResponse");
        }
        if let Self::Response { id: Some(Id::Text(id)), error, .. } = &frame {
            anyhow::ensure!(!id.is_empty(), "InvalidProtocolResponseId");
            anyhow::ensure!(error.as_ref().is_none_or(|e| !e.code.is_empty()), "InvalidProtocolErrorCode");
        }
        if let Self::Event { event: Event::Usage { usage: u }, .. } = &frame {
            anyhow::ensure!([u.input,u.output,u.cache_read,u.cache_write,u.total_tokens,
                u.cost.input,u.cost.output,u.cost.cache_read,u.cost.cache_write,u.cost.total]
                .iter().all(|n| n.is_finite() && *n >= 0.0), "InvalidProtocolUsage");
        }
        Ok(frame)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Auth {
    pub provider: String,
    pub operation: String,
    #[serde(skip_serializing_if = "Option::is_none")] pub input: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog { pub models: Vec<CatalogModel> }
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CatalogModel {
    pub provider: String, pub id: String, pub name: String, pub api: String, pub base_url: String,
    pub input: Vec<ModelInput>, pub cost: ModelCost, pub reasoning: bool,
    pub context_window: u64, pub max_tokens: u64,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelInput { Text, Image }
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelCost {
    pub input: f64, pub output: f64, pub cache_read: f64, pub cache_write: f64,
    #[serde(skip_serializing_if = "Option::is_none")] pub tiers: Option<Vec<ModelCostTier>>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelCostTier {
    pub input: f64, pub output: f64, pub cache_read: f64, pub cache_write: f64, pub input_tokens_above: f64,
}
impl Catalog {
    pub fn parse(value: Value) -> anyhow::Result<Self> {
        let catalog: Self = serde_json::from_value(value)?;
        for m in &catalog.models {
            anyhow::ensure!([&m.provider, &m.id, &m.name, &m.api].iter().all(|s| !s.is_empty())
                && m.context_window > 0 && m.max_tokens > 0
                && [m.cost.input, m.cost.output, m.cost.cache_read, m.cost.cache_write]
                    .iter().all(|n| n.is_finite()), "AgentdInvalidCatalog");
            for tier in m.cost.tiers.iter().flatten() {
                anyhow::ensure!([tier.input, tier.output, tier.cache_read, tier.cache_write, tier.input_tokens_above]
                    .iter().all(|n| n.is_finite()) && tier.input_tokens_above >= 0.0, "AgentdInvalidCatalogTier");
            }
        }
        Ok(catalog)
    }
}

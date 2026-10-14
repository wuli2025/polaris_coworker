//! Exact wire names from agentd/src/protocol/{schema,events}.ts.
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(untagged)]
pub enum Id { Text(String), Number(i64) }
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request { pub id: Id, #[serde(flatten)] pub command: Command }
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum Command {
    #[serde(rename = "ping")] Ping(Empty),
    #[serde(rename = "models.list")] Models(Empty),
    #[serde(rename = "session.open")] Open(Open),
    #[serde(rename = "prompt")] Prompt(Text),
    #[serde(rename = "steer")] Steer(Text),
    #[serde(rename = "followUp")] FollowUp(Text),
    #[serde(rename = "abort")] Abort(Session),
    #[serde(rename = "close")] Close(Session),
    #[serde(rename = "config.apply")] Apply(Apply),
}
#[derive(Debug, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Empty {}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Open {
    #[serde(skip_serializing_if = "Option::is_none")] pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub cwd: Option<String>,
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
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "event")]
pub enum Event {
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

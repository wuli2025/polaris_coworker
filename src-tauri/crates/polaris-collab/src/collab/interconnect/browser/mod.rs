//! HTTP clients get independent, bound session devices, never the server's console identity.
use crate::collab::{auth, db};
use super::login::{self, Progress, Stage, Steps};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{future::Future, pin::Pin};
mod authority;
mod connections;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub device_name: String,
    pub platform: String,
    pub gateway_url: Option<String>,
    pub request_timeout_secs: u64,
    pub max_peers: usize,
}
impl Config {
    pub fn validate(&self) -> Result<(), String> {
        for value in [&self.device_name, &self.platform] { validate_label(value)?; }
        if !(1..=60).contains(&self.request_timeout_secs) || !(1..=128).contains(&self.max_peers) {
            return Err("browser timeout/peer limit out of range".into());
        }
        if let Some(url) = &self.gateway_url { super::validate_url(url)?; }
        Ok(())
    }
}
fn validate_label(value: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.chars().count() > 40 || value.chars().any(char::is_control) {
        return Err("device label must contain 1..40 printable characters".into());
    }
    Ok(())
}
fn string<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value[field].as_str().ok_or_else(|| format!("missing {field}"))
}

#[derive(Clone, Serialize, Deserialize)]
struct Record {
    authority_url: Option<String>,
    uid: String, node: String, key: String, name: String, platform: String,
    progress: Value, discovery: Value, connections: Vec<Value>,
}
impl Record {
    fn save(&self, token: &str) -> Result<(), String> {
        db::with_conn(|conn| {
            conn.execute("INSERT INTO browser_interconnect(token,record) VALUES(?1,?2)
                ON CONFLICT(token) DO UPDATE SET record=excluded.record", params![token, json!(self).to_string()])
                .map_err(|e| e.to_string())?;
            Ok(())
        })
    }
    fn response(&self) -> Value {
        let mut devices = self.connections.clone();
        devices.insert(0, json!({"nodeId":self.node,"name":self.name,"os":self.platform,"isSelf":true,
            "online":true,"connected":true,"mounted":false,"storageMode":"http"}));
        json!({"enrolled":true,"uid":self.uid,"nodeId":self.node,"deviceKind":"session",
            "interconnect":self.progress,"relayConfig":relay_status(),"peers":self.connections,"devices":devices,
            "authorityConfigured":true,"storageMode":"http"})
    }
    fn assertion(&self) -> Result<String, String> {
        let value = authority::call(self, "POST", "/api/mesh/assert", &json!({}))?;
        let assertion = string(&value, "assertion")?;
        let claims = authority::verify(assertion)?;
        if claims.uid != self.uid || claims.node_id.as_deref() != Some(&self.node) || claims.device_id.is_none() {
            return Err("interconnect_device_mismatch".into());
        }
        Ok(assertion.to_string())
    }
}
fn relay_status() -> Value {
    #[cfg(feature = "collab-net")]
    { crate::collab::tunnel::relay_status() }
    #[cfg(not(feature = "collab-net"))]
    { json!({"urls":[],"state":"unavailable","pending":false}) }
}
fn load(token: &str) -> Result<Record, String> {
    auth::check_session(token)?;
    let raw: String = db::with_conn(|conn| conn.query_row(
        "SELECT record FROM browser_interconnect WHERE token=?1", params![token], |r| r.get(0),
    ).map_err(|e| e.to_string()))?;
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LoginInput {
    username: String, password: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    device_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    platform: Option<String>,
}

struct Coordinator {
    record: Option<Record>, token: String, credentials: Option<Value>, persistence_error: Option<String>,
}
impl Steps for Coordinator {
    fn publish(&mut self, progress: &Progress) {
        if let Some(record) = &mut self.record {
            record.progress = json!(progress);
            if let Err(error) = record.save(&self.token) { self.persistence_error = Some(error); }
        }
    }
    fn execute(&mut self, stage: Stage) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + '_>> {
        Box::pin(async move {
            if self.persistence_error.is_some() { return Err("interconnect_state_unavailable".into()); }
            if stage == Stage::Register {
                let input = self.credentials.take().ok_or("interconnect_login_required")?;
                let (token, record) = tokio::task::spawn_blocking(move || register(input)).await
                    .map_err(|_| "interconnect_login_failed")??;
                self.token = token;
                self.record = Some(record);
                return Ok(());
            }
            let mut record = self.record.clone().ok_or("interconnect_login_required")?;
            let (updated, result) = tokio::task::spawn_blocking(move || {
                let result = match stage {
                    Stage::Discover => (|| {
                        record.assertion()?;
                        record.discovery = authority::call(&record, "POST", "/api/mesh/announce",
                            &json!({"name":record.name,"os":record.platform,"ver":env!("CARGO_PKG_VERSION")}))?;
                        Ok(())
                    })(),
                    Stage::Relay => super::apply_received_relays(&record.discovery),
                    Stage::Connect => connections::connect(&mut record),
                    Stage::Register => Err("interconnect_invalid_stage".into()),
                };
                (record, result)
            }).await.map_err(|_| "interconnect_task_failed")?;
            self.record = Some(updated);
            result
        })
    }
}
fn register(value: Value) -> Result<(String, Record), String> {
    let config = super::load()?.browser;
    let name = value.get("deviceName").map_or(Ok(config.device_name.as_str()), |v| v.as_str().ok_or("invalid deviceName"))?;
    let platform = value.get("platform").map_or(Ok(config.platform.as_str()), |v| v.as_str().ok_or("invalid platform"))?;
    validate_label(name)?;
    validate_label(platform)?;
    let (assertion, claims) = authority::password(string(&value, "username")?, string(&value, "password")?)?;
    // Remote admission must succeed before enrollment; no unbound session is ever issued here.
    let admission = if crate::collab::authority::owner_uid() == claims.uid {
        crate::collab::authority::Admission::SelfOwned
    } else { crate::collab::authority::Admission::ExistingOnly };
    let user = crate::collab::authority::upsert_member(&claims, admission)?;
    let node = format!("web_{}", auth::random_token());
    let key = authority::enroll(&assertion, &claims.uid, &node, name, platform)?;
    let authority_url = if crate::collab::authority::is_authority() { None } else { super::load()?.authority.url };
    let record = Record { authority_url, uid: claims.uid, node, key, name: name.into(), platform: platform.into(),
        progress: Value::Null, discovery: Value::Null, connections: vec![] };
    let bound = authority::verify(&record.assertion()?)?;
    let token = super::binding::issue(user.id, &bound, &record.node)?;
    if let Err(error) = record.save(&token) {
        let _ = auth::logout(&token);
        return Err(error);
    }
    Ok((token, record))
}

pub async fn password_login(input: Value) -> Result<Value, String> {
    let input: LoginInput = serde_json::from_value(input).map_err(|_| "interconnect_login_failed")?;
    if input.username.len() > 256 || input.password.len() > 1024 { return Err("interconnect_login_failed".into()); }
    let input = json!(input);
    let mut coordinator = Coordinator { record: None, token: String::new(), credentials: Some(input), persistence_error: None };
    let progress = login::run(&mut coordinator, Stage::Register).await;
    if progress.stage == Stage::Register { return Err("interconnect_login_failed".into()); }
    let record = coordinator.record.ok_or("interconnect_login_failed")?;
    if coordinator.persistence_error.is_some() { return Err("interconnect_state_unavailable".into()); }
    let mut output = record.response();
    output["token"] = json!(coordinator.token);
    Ok(output)
}

pub fn status(token: &str) -> Result<Value, String> {
    let record = load(token)?;
    let assertion = record.assertion()?;
    authority::call(&record, "POST", "/api/mesh/announce",
        &json!({"name":record.name,"os":record.platform,"ver":env!("CARGO_PKG_VERSION")}))?;
    let mut output = record.response();
    output["deviceAssertion"] = json!(assertion);
    Ok(output)
}

pub async fn retry(token: String) -> Result<Value, String> {
    // A weak per-session mutex bounds registry lifetime without serializing unrelated logins.
    let lock = connections::session_lock(&token);
    let _guard = lock.try_lock().map_err(|_| "interconnect_retry_busy")?;
    let record = load(&token)?;
    let mut coordinator = Coordinator { record: Some(record), token, credentials: None, persistence_error: None };
    login::run(&mut coordinator, Stage::Discover).await;
    if coordinator.persistence_error.is_some() { return Err("interconnect_state_unavailable".into()); }
    Ok(coordinator.record.ok_or("interconnect_login_required")?.response())
}

pub fn security(token: &str, path: &str, value: &Value) -> Result<Value, String> {
    let record = load(token)?;
    record.assertion()?;
    authority::call(&record, if path.ends_with("inbox") { "GET" } else { "POST" }, path, value)
}

/// Returns false for pre-existing session types so their existing policy remains in charge.
pub fn risk(token: &str, operation: &str, payload: &Value) -> Result<bool, String> {
    let exists: Option<i64> = db::with_conn(|conn| conn.query_row(
        "SELECT 1 FROM browser_interconnect WHERE token=?1", params![token], |r| r.get(0),
    ).optional().map_err(|e| e.to_string()))?;
    if exists.is_none() { return Ok(false); }
    let record = load(token)?;
    let claims = authority::verify(&record.assertion()?)?;
    let digest = super::risk::digest(operation, payload);
    let nonce = auth::random_token();
    let value = authority::call(&record, "POST", "/api/mesh/risk/authorize",
        &json!({"actorNodeId":record.node,"operation":operation,"digest":digest,"nonce":nonce}))?;
    let key = if crate::collab::authority::is_authority() { crate::collab::authority::public_key_b64()? }
        else { crate::collab::authority::pinned().ok_or("interconnect_authority_required")?.1 };
    let permit: super::risk::Permit = crate::collab::authority::verify_document("PRISK1", string(&value, "permit")?, &key)?;
    if permit.uid != record.uid || permit.host != record.node || Some(permit.actor) != claims.device_id
        || permit.digest != digest || permit.nonce != nonce || permit.expires_at <= db::now()
    { return Err("interconnect_invalid_permit".into()); }
    Ok(true)
}

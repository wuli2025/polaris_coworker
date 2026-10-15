//! Active request ownership and running-input commands. No caller-supplied owner on IPC.
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone)]
struct Binding { owner: String, conversation: Option<String> }
#[derive(Default)]
struct Registry(HashMap<String, Binding>);
static ACTIVE: Lazy<Mutex<Registry>> = Lazy::new(|| Mutex::new(Registry::default()));
pub(crate) struct Lease(String);
impl Drop for Lease {
    fn drop(&mut self) { finished(&self.0); }
}
pub(crate) fn finished(req: &str) { ACTIVE.lock().0.remove(req); }
impl Registry {
    fn insert(&mut self, req: &str, conversation: Option<&str>, owner: &str) -> Result<(), String> {
        if self.0.contains_key(req) || conversation.is_some_and(|id| {
            self.0.values().any(|b| b.conversation.as_deref() == Some(id))
        }) { return Err("ChatBusy".into()); }
        self.0.insert(req.into(), Binding { owner: owner.into(), conversation: conversation.map(String::from) });
        Ok(())
    }
    fn resolve(&self, owner: &str, target: &Target) -> Result<String, String> {
        if target.req_id.is_none() && target.conversation_id.is_none() {
            return Err("ChatTargetRequired".into());
        }
        let found = match &target.req_id {
            Some(req) => self.0.get(req).map(|binding| (req, binding)),
            None => self.0.iter().find(|(_, b)| b.conversation == target.conversation_id),
        };
        let (req, binding) = found.ok_or("ChatNotRunning")?;
        if binding.owner != owner { return Err("ChatForbidden".into()); }
        if target.conversation_id.is_some() && target.conversation_id != binding.conversation {
            return Err("ChatTargetMismatch".into());
        }
        Ok(req.clone())
    }
}
pub(crate) fn reserve(req: &str, conversation: Option<&str>, owner: &str) -> Result<Lease, String> {
    ACTIVE.lock().insert(req, conversation, owner)?;
    Ok(Lease(req.into()))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Target {
    pub req_id: Option<String>,
    pub conversation_id: Option<String>,
    pub text: String,
}
/// HTTP supplies authenticated identity; desktop commands use the local trusted principal.
/// Input is accepted only by the current route. agentd decides whether the turn is still streaming.
pub fn send(owner: &str, target: Target, follow_up: bool) -> Result<Value, String> {
    if target.text.trim().is_empty() { return Err("ChatEmptyInput".into()); }
    if target.text.len() > 2 * 1024 * 1024 { return Err("ChatInputTooLarge".into()); }
    let req = ACTIVE.lock().resolve(owner, &target)?;
    super::backend::selected().and_then(|b| b.live_input(&req, &target.text, follow_up))
        .map_err(|e| e.to_string())
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub async fn chat_steer(req_id: Option<String>, conversation_id: Option<String>, text: String) -> Result<Value, String> {
    tokio::task::spawn_blocking(move || send("desktop", Target { req_id, conversation_id, text }, false))
        .await.map_err(|e| e.to_string())?
}
#[cfg_attr(feature = "desktop", tauri::command)]
pub async fn chat_follow_up(req_id: Option<String>, conversation_id: Option<String>, text: String) -> Result<Value, String> {
    tokio::task::spawn_blocking(move || send("desktop", Target { req_id, conversation_id, text }, true))
        .await.map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn done_releases_binding_before_next_turn_and_old_lease_cannot_remove_it() -> Result<(), String> {
        let old = reserve("lease-old", Some("lease-conv"), "alice")?;
        finished("lease-old");
        let next = reserve("lease-next", Some("lease-conv"), "alice")?;
        drop(old);
        let target = Target { req_id: None, conversation_id: Some("lease-conv".into()), text: "next".into() };
        assert_eq!(ACTIVE.lock().resolve("alice", &target)?, "lease-next");
        drop(next);
        assert_eq!(ACTIVE.lock().resolve("alice", &target), Err("ChatNotRunning".into()));
        Ok(())
    }
    #[test]
    fn ownership_idle_busy_and_stale_request() -> Result<(), String> {
        let mut registry = Registry::default();
        let mut target = Target { req_id: None, conversation_id: Some("conv".into()), text: "hello".into() };
        assert_eq!(registry.resolve("alice", &target), Err("ChatNotRunning".into()));
        registry.insert("req1", Some("conv"), "alice")?;
        assert_eq!(registry.resolve("alice", &target)?, "req1");
        assert_eq!(registry.resolve("bob", &target), Err("ChatForbidden".into()));
        assert_eq!(registry.insert("req2", Some("conv"), "bob"), Err("ChatBusy".into()));
        target.req_id = Some("req1".into());
        assert_eq!(registry.resolve("bob", &target), Err("ChatForbidden".into()));
        target.conversation_id = Some("different".into());
        assert_eq!(registry.resolve("alice", &target), Err("ChatTargetMismatch".into()));
        registry.0.remove("req1");
        registry.insert("req2", Some("conv"), "alice")?;
        assert_eq!(registry.resolve("alice", &target), Err("ChatNotRunning".into()));
        Ok(())
    }
}

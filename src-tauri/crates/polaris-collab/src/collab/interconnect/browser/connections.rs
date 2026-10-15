//! Browser storage uses authenticated HTTP over the existing gateway tunnel, never host OS mounts.
use super::*;
use std::{collections::HashMap, sync::{Arc, Weak}};
type SessionLock = tokio::sync::Mutex<()>;
static LOCKS: once_cell::sync::Lazy<parking_lot::Mutex<HashMap<String, Weak<SessionLock>>>> =
    once_cell::sync::Lazy::new(|| parking_lot::Mutex::new(HashMap::new()));

pub(super) fn session_lock(token: &str) -> Arc<SessionLock> {
    let mut locks = LOCKS.lock();
    locks.retain(|_, lock| lock.strong_count() > 0);
    let key = super::super::risk::digest("browser-retry", &json!(token));
    if let Some(lock) = locks.get(&key).and_then(Weak::upgrade) { return lock; }
    let lock = Arc::new(SessionLock::new(()));
    locks.insert(key, Arc::downgrade(&lock));
    lock
}

pub(super) fn connect(record: &mut Record) -> Result<(), String> {
    connect_with(record, authority::request)
}

fn connect_with(record: &mut Record, mut request: impl FnMut(&str, &str, Option<&str>, &Value) -> Result<Value, String>) -> Result<(), String> {
    let config = super::super::load()?;
    let peers = record.discovery["peers"].as_array().ok_or("interconnect_missing_peers")?;
    if peers.len() > config.browser.max_peers { return Err("interconnect_peer_limit".into()); }
    let gateway = config.browser.gateway_url.or(config.authority.url);
    let assertion = record.assertion()?;
    let previous = std::mem::take(&mut record.connections);
    let mut failed = false;
    for peer in peers {
        let node = string(peer, "node_id")?;
        let mut link = json!({"nodeId":node,"name":peer["name"],"os":peer["os"],
            "online":peer["online"],"connected":false,"mounted":false,"storageMode":"http"});
        if peer["online"] != true {
            link["state"] = json!("offline");
        } else {
            let result = (|| {
                // A directory value is never allowed to turn into a URL path or SSRF target.
                if node.len() != 64 || !node.bytes().all(|c| c.is_ascii_hexdigit()) {
                    return Err("interconnect_invalid_peer".into());
                }
                let base = gateway.as_deref().ok_or("interconnect_gateway_required")?.trim_end_matches('/');
                let url = format!("{base}/h/{node}");
                let previous = previous.iter().find(|item| item["nodeId"] == node && item["url"] == url);
                link["url"] = json!(url);
                if let Some(token) = previous.and_then(|item| item["token"].as_str()) {
                    link["token"] = json!(token);
                    match request("GET", &format!("{url}/api/fs/caps"), Some(token), &Value::Null) {
                        Ok(caps) => return accept_caps(&mut link, caps),
                        Err(error) if error == "interconnect_upstream_unauthorized" => {}
                        Err(error) => return Err(error),
                    }
                }
                let login = request("POST", &format!("{url}/api/collab/login_assertion"), None,
                    &json!({"assertion":assertion,"deviceId":record.node}))?;
                let token = string(&login, "token")?;
                // Keep issued sessions on partial failure; reauthenticate only on explicit rejection.
                link["token"] = json!(token);
                let caps = request("GET", &format!("{url}/api/fs/caps"), Some(token), &Value::Null)?;
                accept_caps(&mut link, caps)?;
                Ok::<(), String>(())
            })();
            if let Err(error) = result {
                link["state"] = json!("failed");
                link["error"] = json!(error);
                failed = true;
            }
        }
        record.connections.push(link);
    }
    if failed { Err("interconnect_peer_connection_failed".into()) } else { Ok(()) }
}

fn accept_caps(link: &mut Value, caps: Value) -> Result<(), String> {
    if caps["read"] != true { return Err("interconnect_storage_denied".into()); }
    link["caps"] = caps;
    link["connected"] = json!(true);
    link["state"] = json!("ready");
    Ok(())
}

#[cfg(test)]
mod tests;

//! Local console policy B adapters and frontend notification endpoints.
use super::*;

pub(super) fn watch_agent(interval: u64) -> Result<ureq::Agent, String> {
    crate::collab::interconnect::http_client::agent(
        ureq::AgentBuilder::new().timeout(Duration::from_secs(interval + 5)))
}

static WATCHING: AtomicBool = AtomicBool::new(false);

pub(super) fn start_watch() {
    if WATCHING.swap(true, Ordering::SeqCst) { return; }
    rt().spawn(async {
        let mut cursor = String::new();
        loop {
            let Some(config) = cfg() else {
                WATCHING.store(false, Ordering::SeqCst);
                return;
            };
            let interval = crate::collab::interconnect::load()
                .map_or(30, |config| config.security.revocation_poll_secs);
            let previous = cursor.clone();
            let response = tokio::task::spawn_blocking(move || -> Result<Value, String> {
                watch_agent(interval)?
                    .get(&format!("{}/api/mesh/security/watch", config.url))
                    .query("cursor", &previous)
                    .set("Authorization", &format!("Bearer {}", config.key)).call()
                    .map_err(cloud_err)?.into_json().map_err(|e| e.to_string())
            }).await;
            match response {
                Ok(Ok(response)) => {
                    if let Some(next) = response["cursor"].as_str() {
                        if next != cursor { publish(&response); }
                        cursor = next.into();
                    } else {
                        eprintln!("[mesh-security] 通知响应缺 cursor");
                        tokio::time::sleep(Duration::from_secs(interval)).await;
                    }
                }
                error => {
                    eprintln!("[mesh-security] 通知连接中断: {error:?}");
                    tokio::time::sleep(Duration::from_secs(interval)).await;
                }
            }
        }
    });
}

pub fn require_local(operation: &str, payload: &Value) -> Result<(), String> {
    if cfg().is_none() { return Ok(()); }
    crate::collab::interconnect::gate::require(&meta(K_UID), &my_node_id(), operation, payload)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub async fn mesh_security_inbox() -> Result<Value, String> {
    call("/api/mesh/security/inbox", None).await
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub async fn mesh_security_ack(device_id: String) -> Result<Value, String> {
    call("/api/mesh/security/ack", Some(json!({"deviceId":device_id}))).await
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub async fn mesh_risk_decide(request_id: String, approve: bool) -> Result<Value, String> {
    call("/api/mesh/risk/decide", Some(json!({"requestId":request_id,"approve":approve}))).await
}

async fn call(path: &'static str, payload: Option<Value>) -> Result<Value, String> {
    let config = cfg().ok_or("本机尚未入网")?;
    tokio::task::spawn_blocking(move || {
        let url = format!("{}{path}", config.url);
        let request = if payload.is_some() { agent()?.post(&url) } else { agent()?.get(&url) }
            .set("Authorization", &format!("Bearer {}", config.key));
        let response = match payload {
            Some(payload) => request.send_json(payload),
            None => request.call(),
        }.map_err(cloud_err)?;
        response.into_json().map_err(|e| format!("安全响应不是 JSON: {e}"))
    }).await.map_err(|e| e.to_string())?
}

pub(super) fn publish(response: &Value) {
    for key in ["notifications", "approvals"] {
        if let Some(events) = response[key].as_array() {
            for event in events {
                #[cfg(feature = "desktop")]
                crate::hosting::bus_emit("interconnect:security", event.clone());
                #[cfg(not(feature = "desktop"))]
                eprintln!("[mesh-security] {event}");
            }
        }
    }
}

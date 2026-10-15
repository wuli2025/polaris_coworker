//! Security polling shares mesh lifecycle entry points but cannot be delayed by peer mounts.
use crate::collab::{authority, db};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

static RUNNING: AtomicBool = AtomicBool::new(false);

pub fn poll_account(url: &str, uid: &str, timeout: Duration) -> Result<(), String> {
    poll_account_with(uid, || {
        ureq::AgentBuilder::new().timeout(timeout).build()
            .get(&format!("{url}/api/mesh/revocations")).query("uid", uid)
            .call().map_err(|e| e.to_string())?.into_json().map_err(|e| e.to_string())
    })
}

pub(super) fn poll_account_with(
    uid: &str, fetch: impl FnOnce() -> Result<serde_json::Value, String>,
) -> Result<(), String> {
    let value = fetch()?;
    let document = value["document"].as_str().ok_or("权威未提供签名吊销清单")?;
    super::revocations::accept(uid, document)
}

pub fn start() {
    let Ok(handle) = tokio::runtime::Handle::try_current() else { return; };
    if RUNNING.swap(true, Ordering::SeqCst) { return; }
    handle.spawn(async {
        loop {
            let config = match super::load() {
                Ok(config) => config,
                Err(error) => {
                    eprintln!("[mesh-security] 配置无效: {error}");
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    continue;
                }
            };
            let next = tokio::time::Instant::now() + Duration::from_secs(config.security.revocation_poll_secs);
            if let Some((url, _, _)) = authority::pinned() {
                match super::revocations::accounts() {
                    Ok(accounts) => {
                        let mut jobs = tokio::task::JoinSet::new();
                        for uid in accounts {
                            let url = url.clone();
                            let timeout = Duration::from_secs(config.security.revocation_poll_secs.min(10));
                            jobs.spawn_blocking(move || {
                                if let Err(error) = poll_account(&url, &uid, timeout) {
                                    eprintln!("[mesh-security] {uid}: 保留最后有效吊销清单: {error}");
                                    db::audit("system", "mesh.revocations.warning", &uid, &error);
                                }
                            });
                        }
                        while let Some(result) = jobs.join_next().await {
                            if let Err(error) = result { eprintln!("[mesh-security] 拉取任务失败: {error}"); }
                        }
                    }
                    Err(error) => eprintln!("[mesh-security] 账号查询失败: {error}"),
                }
            }
            tokio::time::sleep_until(next).await;
        }
    });
}

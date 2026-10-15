//! Shared post-login sequence and resumable background reconciliation.
use super::*;
use crate::collab::interconnect::login::{self as pipeline, Progress, Stage, Steps};
use std::{future::Future, pin::Pin};

static PROGRESS: parking_lot::Mutex<Option<Progress>> = parking_lot::Mutex::new(None);
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub(super) fn status() -> Value { json!(*PROGRESS.lock()) }

pub(super) fn relay_status() -> Value {
    #[cfg(feature = "collab-net")]
    { crate::collab::tunnel::relay_status() }
    #[cfg(not(feature = "collab-net"))]
    { json!({"state": "unavailable", "pending": false}) }
}

struct Enrollment {
    url: String,
    node: String,
    assertion: String,
    invite: String,
    console: bool,
}

#[derive(Default)]
struct Coordinator {
    enrollment: Option<Enrollment>,
    output: Value,
    response: Value,
    count: usize,
}

impl Steps for Coordinator {
    fn publish(&mut self, progress: &Progress) {
        *PROGRESS.lock() = Some(progress.clone());
        #[cfg(feature = "desktop")]
        crate::hosting::bus_emit("interconnect:progress", json!(progress));
    }

    fn execute(&mut self, stage: Stage)
        -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + '_>> {
        Box::pin(async move {
            match stage {
                Stage::Register => {
                    let enrollment = self.enrollment.take().ok_or("缺登录凭据，请重新登录")?;
                    self.output = tokio::task::spawn_blocking(move || {
                        let out = super::login::enroll_with_assertion(
                            &enrollment.url, &enrollment.node, &enrollment.assertion,
                            &enrollment.invite, enrollment.console,
                        )?;
                        super::login::finish_enroll(&enrollment.url, &enrollment.node, &out)
                    }).await.map_err(|e| format!("登记设备任务失败: {e}"))??;
                }
                Stage::Discover => {
                    let config = cfg().ok_or("本机还没入网")?;
                    self.response = tokio::task::spawn_blocking(move || {
                        heal_local_membership(&config);
                        if !crate::collab::authority::is_member(&meta(K_UID)) {
                            return Err("本机成员资格未建立，无法互联".into());
                        }
                        announce(&config)
                    }).await.map_err(|e| format!("拉取设备清单任务失败: {e}"))??;
                }
                Stage::Relay => {
                    crate::collab::interconnect::apply_received_relays(&self.response)?;
                }
                Stage::Connect => {
                    let config = cfg().ok_or("本机还没入网")?;
                    let peers = self.response.get("peers").and_then(Value::as_array)
                        .ok_or("权威响应缺少 peers 数组")?;
                    self.count = reconcile_peers(&config, peers).await?;
                    let failures: Vec<String> = links().lock().map_err(|e| e.to_string())?
                        .values().filter(|link| !link.err.is_empty() || link.phase != "ready")
                        .map(|link| format!("{}: {} {}", link.name, link.phase, link.err)).collect();
                    if !failures.is_empty() { return Err(failures.join("; ")); }
                }
            }
            Ok(())
        })
    }
}

pub(super) async fn complete_login(
    url: String, node: String, assertion: String, invite: String, console: bool,
) -> Result<Value, String> {
    let _serial = SERIAL.lock().await;
    let mut coordinator = Coordinator {
        enrollment: Some(Enrollment { url, node, assertion, invite, console }),
        ..Coordinator::default()
    };
    let progress = pipeline::run(&mut coordinator, Stage::Register).await;
    if progress.stage == Stage::Register && progress.state == "failed" {
        return Err(progress.error.map_or_else(|| "登记设备失败".into(), |error| error));
    }
    // Enrollment has been persisted. Connectivity errors stay explicit and retry in the loop.
    spawn_loop();
    coordinator.output["interconnect"] = json!(progress);
    coordinator.output["relayConfig"] = relay_status();
    Ok(coordinator.output)
}

pub(super) async fn retry() -> Result<usize, String> {
    let _serial = SERIAL.lock().await;
    let mut coordinator = Coordinator::default();
    let progress = pipeline::run(&mut coordinator, Stage::Discover).await;
    if let Some(error) = progress.error { return Err(error); }
    Ok(coordinator.count)
}

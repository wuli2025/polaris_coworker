//! 桌面「把这台电脑设为主机」:进程内嵌 axum,只挂协作端点(/api/collab/*、/git/*、/ws、/api/health)。
//! 不碰 kb/chat 等模块 —— 它们已由桌面 setup 初始化,这里只是给同事开一扇协作门。
//!
//! 端口:默认从 8484 起扫到 8494(避开 8080 —— Docker Desktop/wslrelay 常把
//! localhost:8080 的 IPv6 侧占掉,踩过「连到别人家服务」的坑)。
//! 事件:内嵌服务有自己的 broadcast 频道(远端成员走 /ws 收);另起桥接任务把
//! 事件转发给本机 Tauri UI(主机人自己的看板实时刷新)。
#![cfg(feature = "desktop")]

use crate::collab::http::{collab_router, detect_advertise_urls, CollabState};
use crate::host::AppHandle as BusHandle;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

const PORT_SCAN: std::ops::Range<u16> = 8484..8495;

struct Running {
    port: u16,
    urls: Vec<String>,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    bus: BusHandle,
}

static RUNNING: Mutex<Option<Running>> = Mutex::new(None);

#[derive(Serialize, Deserialize, Clone, Copy, Default)]
struct HostCfg {
    enabled: bool,
    port: Option<u16>,
}

fn cfg_path() -> Option<PathBuf> {
    directories::UserDirs::new().map(|u| u.home_dir().join("Polaris/data/collab_host.json"))
}

fn load_cfg() -> HostCfg {
    cfg_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_cfg(c: HostCfg) {
    if let Some(p) = cfg_path() {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(p, serde_json::to_string(&c).unwrap_or_default());
    }
}

/// 内核启动(不依赖 tauri,可单测):绑端口 → 建独立事件频道 → 起 axum(graceful shutdown)。
pub async fn start_core(
    pref_port: Option<u16>,
) -> Result<(u16, Vec<String>, BusHandle, tokio::sync::oneshot::Sender<()>), String> {
    let (tx, _rx) = tokio::sync::broadcast::channel::<crate::host::Event>(4096);
    let bus = BusHandle::new(tx);
    let auth_token = std::env::var("POLARIS_AUTH_TOKEN").ok().filter(|s| !s.is_empty());

    let mut cands: Vec<u16> = Vec::new();
    if let Some(p) = pref_port {
        cands.push(p);
    }
    cands.extend(PORT_SCAN);
    let mut bound = None;
    for p in cands {
        if let Ok(l) = tokio::net::TcpListener::bind(("0.0.0.0", p)).await {
            bound = Some((l, p));
            break;
        }
    }
    let (listener, port) = bound.ok_or("端口 8484-8494 全被占用,请释放一个再试")?;
    let urls = detect_advertise_urls(port);

    let state = CollabState {
        app: bus.clone(),
        auth_token: Arc::new(auth_token),
        advertise: Arc::new(parking_lot::RwLock::new(urls.clone())),
    };
    // 分享码探活 + 成员端 REST 都是跨源 → CORS 必开;git push 大包 → body 上限同 server 壳。
    let router = collab_router(state, true)
        .route("/api/health", axum::routing::get(|| async { "ok" }))
        .layer(axum::extract::DefaultBodyLimit::max(512 * 1024 * 1024))
        .layer(tower_http::cors::CorsLayer::permissive());

    let (stx, srx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        let _ = axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = srx.await;
            })
            .await;
    });
    Ok((port, urls, bus, stx))
}

fn status_json() -> serde_json::Value {
    let g = RUNNING.lock();
    // is_bootstrap()==true 表示还没有任何账号 → 前端应引导「初始化」而非「登录」。
    let needs_bootstrap = crate::collab::auth::is_bootstrap().unwrap_or(true);
    match g.as_ref() {
        Some(r) => serde_json::json!({
            "running": true, "port": r.port, "urls": r.urls,
            "needsBootstrap": needs_bootstrap, "autostart": load_cfg().enabled,
        }),
        None => serde_json::json!({
            "running": false, "port": 0, "urls": [],
            "needsBootstrap": needs_bootstrap, "autostart": load_cfg().enabled,
        }),
    }
}

/// 事件桥:内嵌服务的广播 → 本机 Tauri UI(主机人自己的看板实时刷新)。
fn bridge_to_ui(app: tauri::AppHandle, bus: &BusHandle) {
    let mut rx = bus.subscribe();
    tauri::async_runtime::spawn(async move {
        use tauri::Emitter;
        while let Ok(ev) = rx.recv().await {
            let _ = app.emit(&ev.topic, ev.payload);
        }
    });
}

#[tauri::command]
pub async fn collab_host_start(app: tauri::AppHandle, port: Option<u16>) -> Result<serde_json::Value, String> {
    if RUNNING.lock().is_some() {
        return Ok(status_json()); // 幂等:已在跑就报状态
    }
    let pref = port.or(load_cfg().port);
    let (port, urls, bus, stx) = start_core(pref).await?;
    bridge_to_ui(app, &bus);
    save_cfg(HostCfg { enabled: true, port: Some(port) });
    *RUNNING.lock() = Some(Running { port, urls, shutdown: Some(stx), bus });
    Ok(status_json())
}

#[tauri::command]
pub fn collab_host_status() -> serde_json::Value {
    status_json()
}

#[tauri::command]
pub fn collab_host_stop() -> Result<serde_json::Value, String> {
    if let Some(mut r) = RUNNING.lock().take() {
        if let Some(s) = r.shutdown.take() {
            let _ = s.send(());
        }
    }
    save_cfg(HostCfg { enabled: false, ..load_cfg() });
    Ok(status_json())
}

/// App 启动时自动拉起(上次开过主机就续上,别让同事早上连不上)。
pub fn auto_start_if_enabled(app: tauri::AppHandle) {
    let cfg = load_cfg();
    if !cfg.enabled {
        return;
    }
    tauri::async_runtime::spawn(async move {
        match start_core(cfg.port).await {
            Ok((port, urls, bus, stx)) => {
                bridge_to_ui(app, &bus);
                *RUNNING.lock() = Some(Running { port, urls, shutdown: Some(stx), bus });
                println!("[collab-host] 主机自启成功,端口 {port}");
            }
            Err(e) => eprintln!("[collab-host] 主机自启失败: {e}"),
        }
    });
}

#[cfg(test)]
mod tests {
    #[tokio::test(flavor = "multi_thread")]
    async fn start_core_serves_health() {
        let _g = crate::collab::db::TEST_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join(format!("collab-host-{}.db", std::process::id()));
        std::env::set_var("POLARIS_COLLAB_DB", &tmp);
        let (port, _urls, _bus, stx) = super::start_core(None).await.expect("start_core");
        // ureq 是既有依赖:同步阻塞客户端,放 spawn_blocking 防塞 runtime。
        let body = tokio::task::spawn_blocking(move || {
            ureq::get(&format!("http://127.0.0.1:{port}/api/health"))
                .call()
                .expect("health call")
                .into_string()
                .expect("body")
        })
        .await
        .unwrap();
        assert_eq!(body, "ok");
        let _ = stx.send(());
        std::env::remove_var("POLARIS_COLLAB_DB");
        let _ = std::fs::remove_file(&tmp);
    }
}

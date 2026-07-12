//! 桌面「把这台电脑设为主机」:进程内嵌 axum,只挂协作端点(/api/collab/*、/git/*、/ws、/api/health)。
//! 不碰 kb/chat 等模块 —— 它们已由桌面 setup 初始化,这里只是给同事开一扇协作门。
//!
//! 端口:默认从 8484 起扫到 8494(避开 8080 —— Docker Desktop/wslrelay 常把
//! localhost:8080 的 IPv6 侧占掉,踩过「连到别人家服务」的坑)。
//! 事件:内嵌服务有自己的 broadcast 频道(远端成员走 /ws 收);另起桥接任务把
//! 事件转发给本机 Tauri UI(主机人自己的看板实时刷新)。
#![cfg(feature = "desktop")]

use crate::collab::apihub::{api_router, ApiState};
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
    /// axum serve 任务句柄:stop 时先给优雅窗口,超时 abort(drop listener 释放端口)。
    /// 只发 oneshot 不收尸的话,/ws 长连接会让 graceful shutdown 永远等不完 →
    /// 旧 serve 僵尸占 8484,再 start 落到 8485 双服务并存,反复停/开逐次泄漏端口。
    serve_task: Option<tokio::task::JoinHandle<()>>,
    /// 事件桥任务句柄(bridge_to_ui):stop 时 abort,防残留任务攒一批订阅者。
    bridge_task: Option<tauri::async_runtime::JoinHandle<()>>,
    /// tauri→bus 的对话流单向桥监听 id:stop 时 unlisten,防重复 start 累积监听。
    chat_bridge: Option<tauri::EventId>,
    /// 存一份 tauri 句柄仅为 stop 时能 unlisten 上面的 chat_bridge。
    app: tauri::AppHandle,
    bus: BusHandle,
    /// 仅经 Tauri IPC 返回给本机 UI，用于首次 bootstrap；绝不挂在 HTTP 路由上。
    access_token: String,
}

static RUNNING: Mutex<Option<Running>> = Mutex::new(None);

#[derive(Serialize, Deserialize, Clone, Copy, Default)]
struct HostCfg {
    enabled: bool,
    port: Option<u16>,
    /// 允许 LAN/Tailscale 直连(绑 0.0.0.0)。打包版双击启动拿不到环境变量,
    /// 所以除 POLARIS_HOST_ALLOW_LAN 外也认这份持久化配置;数据面仍有口令/会话闸。
    #[serde(default)]
    allow_lan: bool,
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

fn random_access_token() -> Result<String, String> {
    use std::fmt::Write as _;
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).map_err(|e| format!("生成主机访问口令失败: {e}"))?;
    let mut token = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(&mut token, "{byte:02x}");
    }
    Ok(token)
}

fn lan_access_enabled() -> bool {
    // 环境变量显式设置时优先(可强制关);未设置则看持久化配置。
    match std::env::var("POLARIS_HOST_ALLOW_LAN") {
        Ok(v) => v == "1" || v.eq_ignore_ascii_case("true"),
        Err(_) => load_cfg().allow_lan,
    }
}

/// 内核启动(不依赖 tauri,可单测):绑端口 → 建独立事件频道 → 起 axum(graceful shutdown)。
/// 返回 serve 任务的 JoinHandle:stop 侧靠它「优雅窗口 + 超时 abort」确认端口真正释放。
#[allow(clippy::type_complexity)]
pub async fn start_core(
    pref_port: Option<u16>,
    api_app: Option<tauri::AppHandle>,
) -> Result<
    (
        u16,
        Vec<String>,
        BusHandle,
        String,
        tokio::sync::oneshot::Sender<()>,
        tokio::task::JoinHandle<()>,
    ),
    String,
> {
    let (tx, _rx) = tokio::sync::broadcast::channel::<crate::host::Event>(4096);
    // tx 需要克隆:一份包成 BusHandle 给 collab 事件面,一份给应用数据面 ApiState.tx 供 /ws 订阅。
    let bus = BusHandle::new(tx.clone());
    let configured_token = std::env::var("POLARIS_AUTH_TOKEN")
        .ok()
        .filter(|s| !s.trim().is_empty());
    let access_token = match configured_token {
        Some(token) => token,
        None => random_access_token()?,
    };
    let auth_token = Some(access_token.clone());

    // 绑定地址:默认只绑 loopback(127.0.0.1)。远端成员经 iroh 隧道入内 —— 隧道有设备白名单,
    // 且把流量转发到本机 127.0.0.1(见 tunnel.rs upstream_addr),loopback 绑定对隧道零影响。
    // 这样同网段的人**无法直连**该端口打到裸明文 HTTP(此前 0.0.0.0 + 未初始化协作会兜底成 owner,
    // 等于对局域网敞开一个 owner 权限的裸接口)。
    // 想让 LAN 内不经隧道、按 IP 直连入伙 → 显式 POLARIS_HOST_ALLOW_LAN=1。即使管理员
    // 没有配置固定口令，本次进程也会生成 256-bit 随机口令，避免裸露 owner 接口。
    let allow_lan = lan_access_enabled();
    let bind_ip: &str = if allow_lan { "0.0.0.0" } else { "127.0.0.1" };

    let mut cands: Vec<u16> = Vec::new();
    if let Some(p) = pref_port {
        cands.push(p);
    }
    cands.extend(PORT_SCAN);
    let mut bound = None;
    for p in cands {
        if let Ok(l) = tokio::net::TcpListener::bind((bind_ip, p)).await {
            bound = Some((l, p));
            break;
        }
    }
    let (listener, port) = bound.ok_or("端口 8484-8494 全被占用,请释放一个再试")?;
    // 把真实端口告知隧道:此前隧道默认转发 8080 而内嵌服务在 8484-8494,
    // 未设 POLARIS_TUNNEL_UPSTREAM 时远端流量会打进黑洞。
    #[cfg(feature = "collab-net")]
    crate::collab::tunnel::set_upstream_port(port);
    let urls = detect_advertise_urls(port);

    // 口令 Arc 在 collab 面与应用数据面之间共享(同一把口令,同一份鉴权语义)。
    let auth_arc = Arc::new(auth_token);
    let state = CollabState {
        app: bus.clone(),
        auth_token: auth_arc.clone(),
        advertise: Arc::new(parking_lot::RwLock::new(urls.clone())),
    };
    // 有 tauri 句柄时挂应用数据面(invoke/upload/file/ws),让远端(手机/中继网关)拿到
    // 与 Docker server 壳一致的完整能力;此时 /ws 由 api_router 统一提供,collab 面不再
    // 单独挂 /ws(否则路由重叠 panic)—— api 的 ws_handler 订阅同一条 bus,collab 事件照收。
    let with_collab_ws = api_app.is_none();
    let mut router = collab_router(state, with_collab_ws)
        .route("/api/health", axum::routing::get(|| async { "ok" }));
    if let Some(app) = api_app {
        let api_state = ApiState {
            app,
            tx: tx.clone(),
            auth_token: auth_arc.clone(),
        };
        router = router.merge(api_router(api_state));
    }
    // 分享码探活 + 成员端 REST 都是跨源 → CORS 必开。普通 JSON 限 2MB；/api/upload
    // 在 api_router 内单独放宽到 512MB;git 路由在 collab_router 内单独放宽。
    let router = router
        .layer(axum::extract::DefaultBodyLimit::max(2 * 1024 * 1024))
        .layer(tower_http::cors::CorsLayer::permissive());

    let (stx, srx) = tokio::sync::oneshot::channel::<()>();
    let serve_task = tokio::spawn(async move {
        let _ = axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = srx.await;
            })
            .await;
    });
    Ok((port, urls, bus, access_token, stx, serve_task))
}

fn status_json() -> serde_json::Value {
    let g = RUNNING.lock();
    // is_bootstrap()==true 表示还没有任何账号 → 前端应引导「初始化」而非「登录」。
    let needs_bootstrap = crate::collab::auth::is_bootstrap().unwrap_or(true);
    match g.as_ref() {
        Some(r) => serde_json::json!({
            "running": true, "port": r.port, "urls": r.urls,
            "needsBootstrap": needs_bootstrap, "autostart": load_cfg().enabled,
            "accessToken": r.access_token,
            "remoteAccess": lan_access_enabled(),
        }),
        None => serde_json::json!({
            "running": false, "port": 0, "urls": [],
            "needsBootstrap": needs_bootstrap, "autostart": load_cfg().enabled,
            "accessToken": "",
            "remoteAccess": false,
        }),
    }
}

/// 事件桥:内嵌服务的广播 → 本机 Tauri UI(主机人自己的看板实时刷新)。
/// Lagged(积压超容被跳帧)必须 continue 而不是退出 —— `while let Ok(..)` 会在第一次
/// Lagged 时整个循环永久死亡,主机本机看板从此不再实时刷新且无任何报错
///(远端成员走 /ws 的 ws_loop 早已是 Lagged=>continue 的正确写法,这里对齐)。
fn bridge_to_ui(app: tauri::AppHandle, bus: &BusHandle) -> tauri::async_runtime::JoinHandle<()> {
    let mut rx = bus.subscribe();
    tauri::async_runtime::spawn(async move {
        use tauri::Emitter;
        use tokio::sync::broadcast::error::RecvError;
        loop {
            match rx.recv().await {
                Ok(ev) => {
                    // chat:stream 由桌面 chat_send 直接 tauri emit 给本机 webview;bus 上的
                    // chat:stream 是 bridge_chat_to_bus 专为远端灌入的,若再 emit 回 tauri 会
                    // 与该桥形成回环(tauri→bus→tauri→…)。故本机 UI 桥跳过它。
                    if ev.topic == "chat:stream" {
                        continue;
                    }
                    let _ = app.emit(&ev.topic, ev.payload);
                }
                Err(RecvError::Lagged(n)) => {
                    eprintln!("[collab-host] UI 事件桥积压,跳过 {n} 条旧事件(继续转发)");
                    continue;
                }
                Err(RecvError::Closed) => break, // 主机已停,频道关闭 → 正常收工
            }
        }
    })
}

/// 对话流单向桥:桌面 chat_send 把 `chat:stream` emit 到 tauri 事件系统(直达本机
/// webview),这里 `listen` 捕获后再灌进内嵌主机的广播 bus → api_router 的 /ws 推给
/// 远端(手机/中继网关)。这样对话逐字流既到本机 UI、也到远端,而 chat_send/pipeline
/// 一行不改。返回监听 id,stop 时 unlisten。
fn bridge_chat_to_bus(app: &tauri::AppHandle, bus: &BusHandle) -> tauri::EventId {
    use tauri::Listener;
    let bus = bus.clone();
    app.listen("chat:stream", move |event| {
        // event.payload() 是已序列化的 JSON 串;解析回 Value 再进 bus(host::AppHandle::emit
        // 会重新序列化并广播为 Event{topic:"chat:stream", payload})。
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(event.payload()) {
            let _ = bus.emit("chat:stream", v);
        }
    })
}

#[tauri::command]
pub async fn collab_host_start(
    app: tauri::AppHandle,
    port: Option<u16>,
) -> Result<serde_json::Value, String> {
    if RUNNING.lock().is_some() {
        return Ok(status_json()); // 幂等:已在跑就报状态
    }
    let pref = port.or(load_cfg().port);
    // 传入 tauri 句柄 → start_core 挂应用数据面(invoke/upload/file/ws);对话流经
    // bridge_chat_to_bus 灌进 bus 供远端 /ws。
    let (port, urls, bus, access_token, stx, serve_task) =
        start_core(pref, Some(app.clone())).await?;
    let bridge_task = bridge_to_ui(app.clone(), &bus);
    let chat_bridge = bridge_chat_to_bus(&app, &bus);
    save_cfg(HostCfg {
        enabled: true,
        port: Some(port),
        ..load_cfg()
    });
    *RUNNING.lock() = Some(Running {
        port,
        urls,
        shutdown: Some(stx),
        serve_task: Some(serve_task),
        bridge_task: Some(bridge_task),
        chat_bridge: Some(chat_bridge),
        app,
        bus,
        access_token,
    });
    Ok(status_json())
}

#[tauri::command]
pub fn collab_host_status() -> serde_json::Value {
    status_json()
}

#[tauri::command]
pub async fn collab_host_stop() -> Result<serde_json::Value, String> {
    // ① 只取走信号与任务句柄,先不清 RUNNING:优雅窗口期间保持「运行中」,
    //    让并发的 start 走幂等分支返回,而不是抢先绑到 8485 造成双服务。
    //    (parking_lot 锁不能跨 await 持有 → 取出后立刻放锁。)
    let (shutdown, serve_task, bridge_task, chat_bridge, app) = {
        let mut g = RUNNING.lock();
        match g.as_mut() {
            Some(r) => (
                r.shutdown.take(),
                r.serve_task.take(),
                r.bridge_task.take(),
                r.chat_bridge.take(),
                Some(r.app.clone()),
            ),
            None => (None, None, None, None, None),
        }
    };
    // ② 发 graceful shutdown 信号(停止 accept 新连接)。
    if let Some(s) = shutdown {
        let _ = s.send(());
    }
    // ③ 事件桥是纯转发循环,直接 abort;对话流桥 unlisten(防重复 start 累积监听)。
    if let Some(bt) = bridge_task {
        bt.abort();
    }
    if let (Some(app), Some(id)) = (app, chat_bridge) {
        use tauri::Listener;
        app.unlisten(id);
    }
    // ④ 优雅窗口:graceful shutdown 会等所有在途连接结束,而 /ws 是长连接永不结束
    //    → 最多等 2s,超时就 abort(drop listener + 连接,端口立即释放),
    //    再短等 1s 确认任务真正退出。否则旧 serve 僵尸占 8484,stop 后立即 start
    //    只能落到 8485,反复停/开会把 8484-8494 逐个泄漏光。
    if let Some(mut h) = serve_task {
        if tokio::time::timeout(std::time::Duration::from_secs(2), &mut h)
            .await
            .is_err()
        {
            h.abort();
            let _ = tokio::time::timeout(std::time::Duration::from_secs(1), &mut h).await;
        }
    }
    // ⑤ 全部确认退出后才清 RUNNING(顺带 drop bus → broadcast 关闭 → 各 ws_loop 收
    //    Closed 自行收尾)。stop 返回后立即 start 能重新绑回 8484。
    *RUNNING.lock() = None;
    save_cfg(HostCfg {
        enabled: false,
        ..load_cfg()
    });
    Ok(status_json())
}

/// App 启动时自动拉起(上次开过主机就续上,别让同事早上连不上)。
pub fn auto_start_if_enabled(app: tauri::AppHandle) {
    let cfg = load_cfg();
    if !cfg.enabled {
        return;
    }
    tauri::async_runtime::spawn(async move {
        match start_core(cfg.port, Some(app.clone())).await {
            Ok((port, urls, bus, access_token, stx, serve_task)) => {
                let bridge_task = bridge_to_ui(app.clone(), &bus);
                let chat_bridge = bridge_chat_to_bus(&app, &bus);
                *RUNNING.lock() = Some(Running {
                    port,
                    urls,
                    shutdown: Some(stx),
                    serve_task: Some(serve_task),
                    bridge_task: Some(bridge_task),
                    chat_bridge: Some(chat_bridge),
                    app,
                    bus,
                    access_token,
                });
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
        // None = 不挂应用数据面(测试无 tauri 句柄),仅验 collab 面 + health 端点。
        let (port, _urls, _bus, _access_token, stx, serve_task) =
            super::start_core(None, None).await.expect("start_core");
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
        // 无 /ws 长连接时 graceful shutdown 应迅速完成;随后同端口必须能立刻重绑
        //(修复2的核心诉求:停机不泄漏端口,stop→start 能回到原端口)。
        let _ = tokio::time::timeout(std::time::Duration::from_secs(3), serve_task).await;
        let l = tokio::net::TcpListener::bind(("0.0.0.0", port))
            .await
            .expect("停机后应能立刻重绑同端口");
        drop(l);
        std::env::remove_var("POLARIS_COLLAB_DB");
        let _ = std::fs::remove_file(&tmp);
    }
}

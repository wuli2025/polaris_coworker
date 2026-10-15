//! Desktop axum assembly with TCP peer metadata for source rate limits.
use super::*;
use crate::apihub::{api_router, ApiState};
use crate::collab::http::{collab_router, detect_advertise_urls, CollabState};

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
            // 恒 false:桌面主机的入场券是分享码里那把口令(上面 access_token),它就在
            // 分享码里、用户复制粘贴即得,不存在「忘了被锁死」的问题,故不走 Docker 那套
            // 免口令。远端成员还经 iroh 隧道转到本机 127.0.0.1,来源判定也认不出谁是谁。
            open_no_auth: false,
        };
        router = router.merge(api_router(api_state.clone()));
        // 应用直投的「粘性回退」:被代理应用页面里的绝对路径(/assets/x.js、/socket.io/…)
        // 不带 /app/{slug} 前缀,靠进门时种的 cookie 认回属于哪个应用。桌面壳根路径本来
        // 就没挂东西(前端是 tauri webview,不由这里托管),接管它零冲突;
        // Docker server 壳根上有 SPA(server.rs 已占 fallback),故这条只在桌面挂。
        let sticky: axum::Router = axum::Router::new().fallback(crate::appproxy::sticky_fallback);
        router = router.merge(sticky);
    }
    // 分享码探活 + 成员端 REST 都是跨源 → CORS 必开。普通 JSON 限 2MB；/api/upload
    // 在 api_router 内单独放宽到 512MB;git 路由在 collab_router 内单独放宽。
    let router = router
        .layer(axum::extract::DefaultBodyLimit::max(2 * 1024 * 1024))
        .layer(tower_http::cors::CorsLayer::permissive());

    // 远程文件浏览(fsface)开放根:设了 POLARIS_FS_ROOTS 才亮,提醒运维已把盘暴露给隧道对端。
    let fs_roots = crate::collab::fsface::roots();
    if !fs_roots.is_empty() {
        eprintln!("[fsface] 远程浏览已开放,根: {fs_roots:?}");
    }

    let (stx, srx) = tokio::sync::oneshot::channel::<()>();
    let serve_task = tokio::spawn(async move {
        let _ = axum::serve(
            listener, router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
            .with_graceful_shutdown(async {
                let _ = srx.await;
            })
            .await;
    });
    Ok((port, urls, bus, access_token, stx, serve_task))
}


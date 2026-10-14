//! Docker(server) 外壳 —— axum HTTP/WS 服务，替代 Tauri 桌面外壳。
//!
//! - `POST /api/invoke {cmd,args}`：把前端 `invoke()` 分发到各引擎模块函数（≈75 命令）。
//! - `GET  /ws`：把各模块 `app.emit(topic,payload)` 广播给浏览器（替代 Tauri event）。
//! - `POST /api/upload`：multipart 上传，替代桌面原生文件对话框（返回服务端临时路径）。
//! - `GET  /api/file?path=`：受限静态文件读取（iframe 预览 / 图片）。
//! - 其余路径：托管打包好的前端 `dist/`（SPA fallback）。
//!
//! 设计要点：引擎模块（kb/chat/conv/...）源码与桌面版**完全相同**，仅外壳不同。

use crate::apihub::{api_router, mime_for, ApiState};
use crate::host::{AppHandle, Event};
use axum::{
    body::Body,
    extract::{DefaultBodyLimit, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use once_cell::sync::Lazy;
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;

#[derive(Clone)]
pub struct AppState {
    pub app: AppHandle,
    pub tx: broadcast::Sender<Event>,
    pub auth_token: Arc<Option<String>>,
    /// 与 ApiState 同名字段同义:没设 POLARIS_AUTH_TOKEN → 免口令。见 apihub::origin_gate。
    pub open_no_auth: bool,
    pub web_dir: PathBuf,
    readiness: Arc<ReadinessState>,
}

#[derive(Clone)]
struct ReadinessState {
    data_root: PathBuf,
    init_errors: Vec<String>,
}

/// 入口：初始化各引擎模块 + 起 axum。由 bin/polaris-server.rs 调用。
pub async fn serve() -> anyhow::Result<()> {
    // 广播频道：所有 emit 走这里 → 全部 WS 订阅者。容量给大些，避免流式 token 丢帧。
    let (tx, _rx) = broadcast::channel::<Event>(16384);
    let app = AppHandle::new(tx.clone());

    // 让 spawn 的 claude CLI 的 cwd 落在数据根 ~/Polaris：项目/KB/产物都在其下，
    // claude 自动信任整棵树。桌面版靠 `CARGO_MANIFEST_DIR` 的父级，但那是编译期路径，
    // 容器运行时不存在 → 这里显式把进程工作目录设到数据根，避免 claude 落到 `/`。
    let data_root = directories::UserDirs::new()
        .map(|u| u.home_dir().join("Polaris"))
        .unwrap_or_else(|| PathBuf::from("/root/Polaris"));
    let mut init_errors = Vec::new();
    if let Err(e) = std::fs::create_dir_all(&data_root) {
        init_errors.push(format!("数据目录不可创建: {e}"));
    }
    if let Err(e) = std::env::set_current_dir(&data_root) {
        init_errors.push(format!("数据目录不可进入: {e}"));
        eprintln!(
            "[polaris-server] 设工作目录失败({}): {e}",
            data_root.display()
        );
    }

    // ── 初始化各模块（与桌面 lib.rs setup 等价，去掉桌面专属部分）──
    if let Err(e) = crate::kb::init(&app) {
        init_errors.push(format!("kb: {e}"));
        eprintln!("[polaris-server] kb::init 失败: {e}");
    }
    // 内核桥注入(kb/fable/expert → chat): 与桌面 setup 同一时机, 任何 chat 请求之前。
    crate::wiring::wire_engine_bridges();
    if let Err(e) = crate::conv::init(&app) {
        init_errors.push(format!("conv: {e}"));
    }
    if let Err(e) = crate::chat::init(&app) {
        init_errors.push(format!("chat: {e}"));
    }
    if let Err(e) = crate::claude_md::init(&app) {
        init_errors.push(format!("claude_md: {e}"));
    }
    if let Err(e) = crate::provider::init(&app) {
        init_errors.push(format!("provider: {e}"));
    }
    crate::skills::seed_video_studio_skill();
    crate::skills::seed_deck_studio_skill();
    crate::skills::seed_web_studio_skill();
    crate::skills::seed_wechat_typesetter_skill();
    crate::skills::seed_wechat_tasks_skill();
    crate::skills::seed_project_check_skill();
    // 注：「请教毛主席」默认隐藏 —— 仅在用户主动安装「毛主席」资料包时装 consult-mao 技能，
    // 启动时不再自动补装（盘上已有数据保留，不删）。
    // 飞书网关「开机自动启动」（若用户开了 auto_start 且凭证齐全）。
    crate::integrations::feishu::auto_start_if_enabled(&app);
    // 寓言计划:感官 API 坞 + 回声层「每日做梦」调度 + 检索枢纽(与桌面 setup 等价)。
    crate::sense::init();
    crate::voice::init();
    crate::echo::start_scheduler(app.clone());
    crate::fable::init();
    // 协作:晨会定时盘点(有主 Agent 的项目每天推 collab:morning 到面板)。
    {
        let app2 = app.clone();
        crate::collab::lead::start_morning_scheduler(move |topic, v| {
            let _ = app2.emit(topic, v);
        });
    }

    let explicit = std::env::var("POLARIS_AUTH_TOKEN")
        .ok()
        .filter(|s| !s.is_empty());
    // 默认**不要访问口令**(2026-07-25 起,整段取舍见 apihub.rs「为什么默认不要访问口令」):
    // NAS 挂在家用路由 NAT 后面、地址还是动态的,公网连不上;而口令没人记得住,忘了又
    // 没有找回入口,实际效果只是把用户锁在自己的软件外面。要上锁走 POLARIS_AUTH_TOKEN
    // (所有来源一律校验)或 POLARIS_REQUIRE_LOGIN=1(按账号管权限),两者都是管理员的
    // 显式动作。POLARIS_ALLOW_OPEN 现在就是默认行为,留着不报错、也不必再设。
    let mut open_no_auth = false;
    let auth_token = match explicit {
        Some(t) => {
            println!("[polaris-server] 已启用访问口令 (POLARIS_AUTH_TOKEN),所有来源一律校验");
            Some(t)
        }
        None => {
            open_no_auth = true;
            if std::env::var("POLARIS_REQUIRE_LOGIN")
                .is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            {
                println!(
                    "[polaris-server] 🔐 POLARIS_REQUIRE_LOGIN=1:未设口令,但所有请求都要登录账号"
                );
            } else if std::env::var("POLARIS_LAN_ONLY")
                .is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            {
                println!("[polaris-server] 🔓 免口令模式(POLARIS_LAN_ONLY=1):局域网 / Tailscale 内直接进,公网来源拒绝");
            } else {
                println!(
                    "[polaris-server] 🔓 免口令模式:任何能连上的来源都可直接使用,不弹口令框。"
                );
                println!("[polaris-server]     要上锁请设 POLARIS_AUTH_TOKEN=<口令>,或 POLARIS_REQUIRE_LOGIN=1 走账号登录。");
            }
            None
        }
    };

    let web_dir = std::env::var("POLARIS_WEB_DIR").unwrap_or_else(|_| "/srv/web".to_string());
    let web_dir = PathBuf::from(web_dir);

    let auth_token = Arc::new(auth_token);
    let state = AppState {
        app: app.clone(),
        tx: tx.clone(),
        auth_token: auth_token.clone(),
        open_no_auth,
        web_dir: web_dir.clone(),
        readiness: Arc::new(ReadinessState {
            data_root,
            init_errors,
        }),
    };
    // 应用数据面(invoke/upload/file/ws)已抽至 crate::apihub(双壳共用);
    // server 壳与桌面 hosting 各自构造 ApiState 并 merge api_router。
    let api_state = ApiState {
        app: app.clone(),
        tx: tx.clone(),
        auth_token: auth_token.clone(),
        open_no_auth,
    };

    let port: u16 = std::env::var("POLARIS_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8080);
    // 协作面状态(crate::collab::http,双壳共用):与 AppState 共享事件广播与口令;
    // advertise = 票据分享码里带的「主机可达地址」,成员端凭码零填写入伙。
    let collab_state = crate::collab::http::CollabState {
        app: state.app.clone(),
        auth_token: state.auth_token.clone(),
        advertise: Arc::new(parking_lot::RwLock::new(
            crate::collab::http::detect_advertise_urls(port),
        )),
    };

    let app_router = Router::new()
        .route("/api/health", get(health))
        .route("/api/live", get(health))
        .route("/api/ready", get(ready))
        .route("/api/build", get(build))
        .route("/api/status", get(status))
        .fallback(get(spa_fallback))
        .with_state(state)
        // 应用数据面(invoke/upload/file/ws)—— 抽出的双壳共用路由,已自带 ApiState。
        .merge(api_router(api_state))
        // /api/collab/* 与 /git/* 全部路由(crate::collab::http,双壳共用)。
        // 三者都是 Router<()>(各自 with_state),merge 在后;layer 在 merge 之后 →
        // 下面的 body 上限与 CORS 同样罩住数据面与协作端点。
        //
        // 那层 from_fn:把「本机免口令 + 这条请求的来源够格」算好塞进请求扩展，
        // 供 collab_bootstrap 读(见 collab::http::OpenGate)。免口令的机器上没有口令
        // 可出示，不给这条闸就永远建不出第一个账号 —— 协作/团队/互联整条线锁死。
        // 判定与 /api/invoke 同源，所以 POLARIS_LAN_ONLY=1 时公网来源同样进不来。
        .merge(
            crate::collab::http::collab_router(collab_state, false).layer(
                axum::middleware::from_fn(
                    move |req: axum::extract::Request, next: axum::middleware::Next| async move {
                        let peer = req
                            .extensions()
                            .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
                            .map(|c| c.0);
                        let gate = crate::apihub::server_bootstrap_gate(
                            open_no_auth,
                            peer,
                            req.headers(),
                        );
                        let mut req = req;
                        req.extensions_mut()
                            .insert(crate::collab::http::OpenGate(gate.is_open()));
                        next.run(req).await
                    },
                ),
            ),
        )
        // 普通 JSON/表单统一 2MB；只有 /api/upload 单独放宽到 512MB。此前把 512MB
        // 套在整棵路由上，未认证登录/bootstrap 也能迫使服务缓冲巨型请求体。
        .layer(DefaultBodyLimit::max(2 * 1024 * 1024))
        // 桌面客户端(Tauri webview,源 http://tauri.localhost)连远端主机是跨源请求;
        // 用 Bearer token 鉴权(非 cookie),故 permissive(允许任意 Origin、方法、头,
        // 不放行凭证)即可,顺带自动处理 OPTIONS 预检。同源的 Docker/Web 前端不受影响。
        .layer(CorsLayer::permissive());

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    // 隧道上游对齐:服务器壳的真实端口告知 tunnel(与 hosting.rs 同理)。
    #[cfg(feature = "collab-net")]
    crate::collab::tunnel::set_upstream_port(port);
    // 主机隧道自启:没有它,容器每次 recreate 之后都得有人手动 POST 一次
    // /api/collab/tunnel/start,否则远程盘一侧只见「隧道已建」却永远等不到应答
    //(一天踩过两次,而且现象是「盘挂不上」,根本不像是少了一条启动指令)。
    // 默认关(桌面壳自有启停 UI,云端服务器才需要开机就在线),`POLARIS_TUNNEL_AUTOSTART=1` 开。
    #[cfg(feature = "collab-net")]
    if std::env::var("POLARIS_TUNNEL_AUTOSTART")
        .map(|v| v.trim() == "1" || v.trim().eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        match crate::collab::tunnel::start_host_blocking_thread() {
            Some(_) => println!("[polaris-server] iroh 主机隧道已随进程自启"),
            None => println!("[polaris-server] iroh 主机隧道已在运行,跳过自启"),
        }
    }
    println!(
        "[polaris-server] 监听 http://0.0.0.0:{port} (前端目录: {})",
        web_dir.display()
    );

    let listener = tokio::net::TcpListener::bind(addr).await?;
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let mut server_task = tokio::spawn(async move {
        // into_make_service_with_connect_info:把 TCP 对端地址塞进请求扩展,鉴权层据此
        // 判「内网免口令」(apihub::origin_gate)。少了它 ConnectInfo 恒 None → 一律按公网
        // 处理 → 家用 NAS 用户照样被口令挡在门外,这条修复整个失效。
        axum::serve(
            listener,
            app_router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .with_graceful_shutdown(async {
            let _ = shutdown_rx.await;
        })
        .await
    });
    tokio::select! {
        joined = &mut server_task => {
            joined.map_err(|e| anyhow::anyhow!("HTTP 服务任务异常: {e}"))??;
        }
        _ = shutdown_signal() => {
            println!("[polaris-server] 收到退出信号，停止接收新请求并保存状态…");
            let _ = shutdown_tx.send(());
            // 先停止会继续产生写入/子进程的集成，再落盘对话状态。
            let _ = crate::integrations::feishu::feishu_gateway_stop(app.clone());
            crate::collab::gitea::stop();
            crate::conv::flush();
            if tokio::time::timeout(Duration::from_secs(15), &mut server_task)
                .await
                .is_err()
            {
                eprintln!("[polaris-server] 15 秒内仍有长连接未退出，强制结束服务任务");
                server_task.abort();
                let _ = server_task.await;
            }
        }
    }
    crate::conv::flush();
    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("install SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = terminate.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

#[path = "apihub/server_status.rs"]
mod runtime_status;
use runtime_status::status;

/// 公开存活探针必须是常量时间、无外部 IO，避免探针本身成为阻塞池 DoS 放大器。
/// 详细依赖/资源状态由 owner-only 且带缓存的 `/api/status` 提供。
async fn health() -> Response {
    "ok".into_response()
}

/// 公开构建身份只含版本与一次启动随机性，不泄露部署配置。更新页靠它区分
/// “旧连接重新连上”与“新容器已经以目标 revision 启动”。
async fn build() -> Json<Value> {
    Json(crate::apihub::docker_build_identity())
}

static READY_CACHE: Lazy<tokio::sync::Mutex<Option<(Instant, bool)>>> =
    Lazy::new(|| tokio::sync::Mutex::new(None));

/// 不泄露环境详情的就绪探针：核心初始化成功、前端入口存在、数据卷可真实写入、
/// 协作 SQLite 可打开。结果短时缓存并单航班执行，避免公开探针放大磁盘/线程池压力。
async fn ready(State(state): State<AppState>) -> Response {
    let mut cache = READY_CACHE.lock().await;
    if let Some((at, is_ready)) = cache.as_ref() {
        if at.elapsed() < Duration::from_secs(10) {
            return if *is_ready {
                "ready".into_response()
            } else {
                (StatusCode::SERVICE_UNAVAILABLE, "not ready").into_response()
            };
        }
    }
    let readiness = state.readiness.clone();
    let web_dir = state.web_dir.clone();
    let is_ready = tokio::task::spawn_blocking(move || readiness_check(&readiness, &web_dir))
        .await
        .unwrap_or(false);
    *cache = Some((Instant::now(), is_ready));
    if is_ready {
        "ready".into_response()
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "not ready").into_response()
    }
}

fn readiness_check(state: &ReadinessState, web_dir: &Path) -> bool {
    if !state.init_errors.is_empty() || !web_dir.join("index.html").is_file() {
        return false;
    }
    let probe = state
        .data_root
        .join(format!(".polaris-readiness-{}", std::process::id()));
    let wrote = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&probe)
        .and_then(|mut f| {
            use std::io::Write as _;
            f.write_all(b"ready")?;
            f.sync_all()
        })
        .is_ok();
    let _ = std::fs::remove_file(&probe);
    if !wrote {
        return false;
    }
    crate::collab::db::open_db()
        .and_then(|conn| {
            conn.query_row("SELECT 1", [], |_| Ok(()))
                .map_err(|e| e.to_string())
        })
        .is_ok()
}

// ───────────────────────── 前端静态托管（SPA fallback）─────────────────────────

async fn spa_fallback(State(state): State<AppState>, uri: axum::http::Uri) -> Response {
    let rel = uri.path().trim_start_matches('/');
    // 未匹配的 /api/* 路径绝不回退到 index.html: 否则手机/中继端请求到拼错或版本不符
    // 的接口会拿到 200 + 整页 HTML, JSON 解析炸掉且无从判断"接口不存在 vs 网络故障"。
    // 明确回 404 JSON, 让客户端能干净地报"未知接口"。
    if rel == "api" || rel.starts_with("api/") {
        return (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
            format!("{{\"error\":\"未知接口: /{rel}\"}}"),
        )
            .into_response();
    }
    // 安全闸: rel 取自原始 URL, 裸 socket 客户端能塞 `../../etc/passwd`(hyper 不规范化
    // `..` 段)。任一段为 `..` 或绝对/盘符前缀 → 当 SPA 路由回 index.html, 绝不拼出 web_dir。
    let traversal = rel.split(['/', '\\']).any(|seg| seg == "..")
        || Path::new(rel).is_absolute()
        || rel.contains(':');
    let mut candidate = if traversal {
        state.web_dir.join("index.html")
    } else {
        state.web_dir.join(rel)
    };
    // 目录或不存在 → 回 index.html（SPA 路由）。
    if rel.is_empty() || !candidate.is_file() {
        candidate = state.web_dir.join("index.html");
    }
    // 双保险: canonicalize 后必须仍落在 web_dir 内(防符号链接/漏网的相对段)。
    if let (Ok(canon), Ok(root)) = (
        std::fs::canonicalize(&candidate),
        std::fs::canonicalize(&state.web_dir),
    ) {
        if !crate::kb::path_contains(&root, &canon) {
            candidate = state.web_dir.join("index.html");
        }
    }
    match tokio::fs::read(&candidate).await {
        Ok(bytes) => {
            let ct = mime_for(&candidate);
            Response::builder()
                .header(header::CONTENT_TYPE, ct)
                .body(Body::from(bytes))
                .unwrap()
        }
        Err(_) => (StatusCode::NOT_FOUND, "前端资源缺失").into_response(),
    }
}

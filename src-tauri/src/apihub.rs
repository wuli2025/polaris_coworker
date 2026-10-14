//! collab/apihub.rs —— 应用数据面 HTTP(invoke / upload / file / ws),**双壳共用**。
//!
//! 从 server.rs 抽出,让「桌面内嵌主机(hosting.rs)」也能对远端(手机/中继网关)提供
//! 与 Docker server 壳**完全一致**的应用能力:≈200 条命令分发、文件上传/预览、事件流。
//! server.rs 仅保留壳专属部分(前端托管 SPA、/api/status 水位、就绪探针)。
//!
//! 事件句柄 `AppHandle` 双壳二选一(与 chat/pipeline.rs 同款):
//!  - server 壳:`crate::host::AppHandle`(broadcast shim),命令 emit 直接进 `tx` → /ws。
//!  - desktop 主机:`tauri::AppHandle`,命令 emit 进 tauri 事件系统;hosting.rs 另架
//!    一座 `tauri.listen("chat:stream") → bus` 单向桥把对话流灌进 `tx`,再经 /ws 送手机。
//!
//! 鉴权沿用 server.rs 原语义(基础面宽松:无口令则合成 owner;真会话 token 升级真实身份)。
#![cfg(feature = "collab-host")]

#[cfg(not(feature = "desktop"))]
use crate::host::AppHandle;
#[cfg(feature = "desktop")]
use tauri::AppHandle;

use crate::collab::http::{bearer_of, err_resp, ok, role_rank, ws_loop, AuthCtx};
use crate::host::Event;
use axum::{
    body::Body,
    extract::{ConnectInfo, DefaultBodyLimit, Multipart, Query, State, WebSocketUpgrade},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::broadcast;

/// 应用数据面所需的最小状态(AppState 的子集):事件句柄 + 广播发送端 + 访问口令。
/// server 壳里 app 与 tx 是同一条广播;desktop 主机里 app=tauri、tx=hosting bus(靠桥连通)。
#[derive(Clone)]
pub struct ApiState {
    pub app: AppHandle,
    pub tx: broadcast::Sender<Event>,
    pub auth_token: Arc<Option<String>>,
    /// 「免口令」总闸(见 origin_gate)。仅 Docker server 壳在**管理员没显式设**
    /// POLARIS_AUTH_TOKEN 时置 true —— 家用 NAS 走的就是这一档:任何能连上的来源
    /// 直接进,不弹口令框(理由见下面「为什么默认免口令」)。桌面 hosting 恒 false:
    /// 它有自己的分享码口令,隧道对端流量还会被转成 127.0.0.1,不能按来源放行。
    pub open_no_auth: bool,
}

impl ApiState {
    fn app(&self) -> AppHandle {
        self.app.clone()
    }
}

/// 应用数据面路由(invoke/upload/file/ws)。返回 `Router<()>`(已 with_state),
/// 供 server.rs 与 hosting.rs 各自 `.merge()` 进主路由。
/// /api/upload 单挂 512MB body 上限;其余端点由调用方整体 2MB 上限约束。
pub fn api_router(state: ApiState) -> Router {
    Router::new()
        .route("/api/auth/state", get(auth_state))
        .route("/api/auth/setup", post(auth_setup))
        .route("/api/invoke", post(invoke))
        .route(
            "/api/upload",
            post(upload).layer(DefaultBodyLimit::max(512 * 1024 * 1024)),
        )
        .route("/api/file", get(serve_file))
        .route("/api/exec", post(exec_ep))
        .route("/ws", get(ws_handler))
        .with_state(state.clone())
        // 应用直投:/app/{slug}/… 反代到本机在跑的 HTTP 应用(自带鉴权与 owner 闸)。
        .merge(crate::appproxy::routes(state))
}

// ───────────────────────── 鉴权 ─────────────────────────
//
// 基础应用面(对话/知识库/文件…)与 collab 面**分开**:维持历史语义——没设全局口令就
// 开放(合成 owner),避免"有人建了协作账号就把整个 App 锁死"。真会话 token 仍会升级成
// 真实身份并受命令角色闸约束。多用户部署要连基础命令也强制登录时设 POLARIS_REQUIRE_LOGIN=1。

/// 是否显式要求登录。抽出来是因为 exec 端点要独立判「开放模式」——
/// 开放模式会合成 owner 全放行,那对读接口尚可,对远程执行等于无鉴权 shell。
fn require_login_env() -> bool {
    std::env::var("POLARIS_REQUIRE_LOGIN")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

// ── 为什么默认不要访问口令(2026-07-25 起)──────────────────────────────────
//
// 访问口令这套东西在家用 NAS 上是**净负债**:
//  · NAS 挂在家用路由的 NAT 后面,拿到的是运营商动态分配、还常带 CGNAT 的地址,没做
//    端口映射时公网压根连不上;真要远程用,走的是 P2P/Tailscale 那条隧道,那条自带身份;
//  · 口令又最容易忘 —— 网页向导让人随手设一个,几周后没人记得,而且没有找回入口,
//    结果是用户被自己的软件锁在门外(这已经是第二次栽在口令上:上一次是随机生成的)。
// 所以现在:**没设 POLARIS_AUTH_TOKEN = 谁都不用口令**,不分内外网;网页里那个「首次
// 设置口令」向导一并撤掉(见 auth_setup),历史落盘的口令也不再生效 —— 否则老用户升级
// 上来照样被自己几周前随手设的那串东西挡在门外,这次改动就白做了。
//
// 要重新上锁只有一条明路,且必须是管理员的显式动作:
//  · `POLARIS_AUTH_TOKEN=<口令>` —— 所有来源一律校验;
//  · `POLARIS_REQUIRE_LOGIN=1`  —— 走账号体系,按人管权限。
// `/api/exec`(远程 shell)不吃免口令这条豁免,没真凭据永远拒绝 —— 见 exec_ep。

/// 生效口令 = **只认环境变量**。None = 这台机器不设口令(免口令模式)。
pub(crate) fn effective_token(env_token: &Option<String>) -> Option<String> {
    env_token.clone()
}

/// 反代场景下是否信任 X-Forwarded-For。默认不信:见 origin_gate 的取舍。
fn trust_proxy_env() -> bool {
    std::env::var("POLARIS_TRUST_PROXY")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// 私网/回环/本机直连网段 —— 这些地址不可能从公网路由过来。
/// 100.64.0.0/10 是 CGNAT 段,Tailscale 的 100.x 也落在这里(tailnet 自带设备白名单)。
fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            v4.is_loopback()
                || v4.is_private() // 10/8, 172.16/12, 192.168/16
                || v4.is_link_local() // 169.254/16
                || (o[0] == 100 && (64..=127).contains(&o[1])) // 100.64/10 CGNAT + Tailscale
        }
        IpAddr::V6(v6) => {
            // IPv4-mapped(::ffff:a.b.c.d)要拆回 v4 判,否则内网 v4 经 v6 套接字进来会被当公网。
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_private_ip(IpAddr::V4(v4));
            }
            let seg = v6.segments();
            v6.is_loopback()
                || (seg[0] & 0xfe00) == 0xfc00 // fc00::/7 ULA
                || (seg[0] & 0xffc0) == 0xfe80 // fe80::/10 link-local
        }
    }
}

/// 免口令模式下**是否还要看来源**。默认 0 = 不看(谁连上谁能用,家用 NAS 的日常)。
/// 显式 `POLARIS_LAN_ONLY=1` 才退回老行为:只放行内网来源、公网来源一律拒。
/// 给「机器确实有公网入口、又不想设口令」的部署留的中间档,普通 NAS 用户碰不到。
fn lan_only_env() -> bool {
    std::env::var("POLARIS_LAN_ONLY")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// 一次请求的免口令判定。分两半:`enabled` 是本壳允不允许走这条路(见
/// ApiState::open_no_auth),`origin_ok` 是这一条请求的来源够不够格
/// —— 默认恒 true,只有 POLARIS_LAN_ONLY=1 时才真去判内外网。
#[derive(Clone, Copy)]
pub(crate) struct OriginGate {
    enabled: bool,
    origin_ok: bool,
}

impl OriginGate {
    /// 这一条请求能不能吃「免口令」这条豁免。协作面的 bootstrap 闸复用它，
    /// 好让两个面用的是同一把尺子（含 POLARIS_LAN_ONLY 的内外网判定）。
    /// 只有 server 壳(与单测)用得到：桌面壳不走 bootstrap 免口令那条路。
    #[cfg(any(feature = "server", test))]
    pub(crate) fn is_open(&self) -> bool {
        self.enabled && self.origin_ok
    }
}

impl OriginGate {
    /// 关死:显式设过口令的部署、exec 这类高危端点、以及一切拿不准来源的地方走它。
    pub(crate) fn closed() -> Self {
        Self {
            enabled: false,
            origin_ok: false,
        }
    }
    fn allows(&self) -> bool {
        self.enabled && self.origin_ok
    }
}

/// 判来源。默认直接放行(免口令 = 不分内外网);只有 POLARIS_LAN_ONLY=1 时才按下面
/// 三条**保守优先**地判内网:
///  ① 拿不到对端地址(没挂 ConnectInfo / 非 TCP)→ 当公网,要口令。
///  ② 带了 X-Forwarded-For / X-Real-IP 说明前面有反代,此时对端 IP 恒是反代自己
///     (常是 127.0.0.1),按它判会把全世界当内网 —— 除非运维显式 POLARIS_TRUST_PROXY=1
///     声明这层反代可信,否则一律当公网。云机走 install-cloud.sh 必写死 POLARIS_AUTH_TOKEN,
///     enabled 本就是 false,这条只是再兜一层。
///  ③ 其余按 TCP 对端地址判私网。
fn origin_gate(enabled: bool, peer: Option<SocketAddr>, headers: &HeaderMap) -> OriginGate {
    // 管理员显式要求登录 = 要按人管权限,别在这儿偷偷放行。
    origin_gate_with(
        enabled && !require_login_env(),
        lan_only_env(),
        peer,
        headers,
    )
}

/// origin_gate 的纯函数内核(两个开关都由调用方传入)。测试直接打这层,免得改进程级
/// 环境变量跟并行跑的其它用例互踩。
fn origin_gate_with(
    enabled: bool,
    lan_only: bool,
    peer: Option<SocketAddr>,
    headers: &HeaderMap,
) -> OriginGate {
    if !lan_only {
        return OriginGate {
            enabled,
            origin_ok: true,
        };
    }
    let forwarded = headers
        .get("x-forwarded-for")
        .or_else(|| headers.get("x-real-ip"));
    let client_ip: Option<IpAddr> = match (forwarded, trust_proxy_env()) {
        (Some(v), true) => v
            .to_str()
            .ok()
            .and_then(|s| s.split(',').next())
            .and_then(|s| s.trim().parse().ok()),
        (Some(_), false) => None, // 有反代但未声明可信 → 判不了,当公网
        (None, _) => peer.map(|a| a.ip()),
    };
    OriginGate {
        enabled,
        origin_ok: client_ip.map(is_private_ip).unwrap_or(false),
    }
}


// Item includes keep the existing dispatcher namespace and feature gates.
include!("apihub/origin_tests.rs");
include!("apihub/auth.rs");
include!("apihub/invoke.rs");
include!("apihub/desktop.rs");
include!("apihub/args.rs");
include!("apihub/build.rs");
include!("apihub/registry.rs");
include!("apihub/update.rs");
include!("apihub/websocket.rs");
include!("apihub/upload.rs");
include!("apihub/files.rs");
include!("apihub/access_tests.rs");
include!("apihub/update_tests.rs");
include!("apihub/registry_tests.rs");
include!("apihub/engines.rs");
include!("apihub/chat_services.rs");

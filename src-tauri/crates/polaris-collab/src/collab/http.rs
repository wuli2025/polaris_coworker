//! 多人协作 HTTP 路由 —— 双壳共用(Docker server 壳 merge;桌面 hosting 内嵌)。
//!
//! 从 server.rs 整体迁入(逻辑零改动,只把状态从 AppState 换成 CollabState):
//! 鉴权胶水(AuthCtx/resolve_auth/角色闸)+ 全部 /api/collab/* handler + /git/* 反代
//! + WS 推流循环。server 壳经 `collab_router(state, false)` merge 复用;桌面
//! 「一键当主机」(collab/hosting.rs)经 `collab_router(state, true)` 内嵌并自带 /ws。
//!
//! 双轨鉴权(向后兼容):
//!  ① 多用户会话(collab):token 命中 sessions 表 → 得到具体用户与角色,命令过角色闸。
//!  ② 传统全局口令 POLARIS_AUTH_TOKEN:命中即视为 owner(机器管理员,单人 Docker 场景)。
//!  协作启用后(users 表非空)未带任何有效凭据 → 拒绝;未启用协作则维持旧语义。

use crate::host::{AppHandle, Event};
use axum::{
    body::Body,
    extract::{ws::Message, ws::WebSocket, Query, State, WebSocketUpgrade},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use once_cell::sync::Lazy;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;

mod account_routes;
mod browser_routes;
use browser_routes::*;
mod mesh_routes;
mod security_routes;
use security_routes::*;
#[cfg(test)]
mod interconnect_tests;
#[cfg(test)]
mod browser_tests;
#[cfg(test)]
mod security_tests;
use account_routes::*;
use mesh_routes::*;

static LEAD_AI_RUNNING: Lazy<parking_lot::Mutex<HashSet<i64>>> =
    Lazy::new(|| parking_lot::Mutex::new(HashSet::new()));

struct LeadAiRunGuard(i64);

impl LeadAiRunGuard {
    fn acquire(project_id: i64) -> Result<Self, String> {
        let mut running = LEAD_AI_RUNNING.lock();
        if !running.insert(project_id) {
            return Err("该项目已有一个主 Agent 请求在运行，请等待完成后重试".into());
        }
        Ok(Self(project_id))
    }
}

impl Drop for LeadAiRunGuard {
    fn drop(&mut self) {
        LEAD_AI_RUNNING.lock().remove(&self.0);
    }
}

/// 协作面状态:与 server::AppState 解耦,桌面内嵌时独立构造。
#[derive(Clone)]
pub struct CollabState {
    /// 事件广播壳(server 壳与其 AppState.app 同源;桌面 hosting 独立频道)
    pub app: AppHandle,
    /// 全局访问口令(POLARIS_AUTH_TOKEN);None = 未设
    pub auth_token: Arc<Option<String>>,
    /// 票据分享码携带的「本机可达地址」(http://ip:port),外壳启动时注入
    pub advertise: Arc<parking_lot::RwLock<Vec<String>>>,
}

/// 探测本机可对外通告的地址。优先 POLARIS_ADVERTISE_URL(逗号分隔,给反代/固定域名用);
/// 否则 UDP connect 技巧取「默认路由出口 IP」与「Tailscale 口 IP」(connect 不发包,零依赖)。
pub fn detect_advertise_urls(port: u16) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(v) = std::env::var("POLARIS_ADVERTISE_URL") {
        for s in v.split(',') {
            let s = s.trim();
            if !s.is_empty() {
                out.push(s.trim_end_matches('/').to_string());
            }
        }
        if !out.is_empty() {
            return out;
        }
    }
    let probe = |target: &str| -> Option<std::net::IpAddr> {
        let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
        s.connect(target).ok()?;
        s.local_addr().ok().map(|a| a.ip())
    };
    let mut ips: Vec<std::net::IpAddr> = Vec::new();
    if let Some(ip) = probe("8.8.8.8:80") {
        ips.push(ip); // 默认路由(局域网/公网口)
    }
    if let Some(ip) = probe("100.100.100.100:80") {
        if !ips.contains(&ip) {
            ips.push(ip); // Tailscale 口(100.64/10 路由存在才命中)
        }
    }
    for ip in ips {
        if !ip.is_loopback() && !is_tun_fake_ip(&ip) {
            out.push(format!("http://{ip}:{port}"));
        }
    }
    out
}

/// 198.18.0.0/15 是基准测试保留段,被 Clash/Surge 等 TUN 代理拿来当 fake-ip。
/// 开着 TUN 时「默认路由出口 IP」探到的就是它;把它通告进连接码,对方(往往也开着
/// TUN)连它会回环到**对方自己**的 Polaris,表现为 401/串机迷惑 —— 一律不通告。
fn is_tun_fake_ip(ip: &std::net::IpAddr) -> bool {
    matches!(ip, std::net::IpAddr::V4(v) if v.octets()[0] == 198 && (v.octets()[1] & 0xfe) == 18)
}

// ───────────────────────── 鉴权 ─────────────────────────

/// 一次请求的鉴权结果:是谁、什么角色。
#[derive(Clone)]
pub struct AuthCtx {
    pub user_id: i64, // 0 = 合成身份(全局口令 admin / 本机单人 local)
    pub username: String,
    pub role: String, // owner|collaborator|visitor|lead
    /// 这条请求是**哪台设备**发来的(会话上记的 device_id;全局口令等合成身份为空)。
    /// 盘与远程执行的按设备闸靠它 —— 见 `grants::fs_gate` / `grants::exec_gate`。
    pub device_id: String,
}

pub fn bearer_of(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.strip_prefix("Bearer ").unwrap_or(s).to_string())
        .or_else(|| {
            headers
                .get("x-polaris-token")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
        })
}

/// 「这台机器处于免口令模式，且**这一条请求**的来源够格」——由外壳(server.rs)按它
/// 自己那套来源判定算好，经请求扩展塞进来。没有这个扩展(桌面内嵌主机 / 老调用方)
/// 一律当 false。
///
/// 只有 [`collab_bootstrap`] 读它：免口令的 NAS 上机器根本没有口令可出示，
/// 不认这条闸就永远建不出第一个账号 —— 协作/团队整条线直接锁死。
/// 其余端点照旧只认真会话，`resolve_auth` 也绝不因此合成 owner。
#[derive(Clone, Copy, Debug)]
pub struct OpenGate(pub bool);

/// token(header 或 ws query) → AuthCtx。None = 未授权。
pub fn resolve_auth(auth_token: &Option<String>, token: Option<&str>) -> Option<AuthCtx> {
    // 轨② 全局口令 = owner。
    if let Some(expected) = auth_token.as_ref() {
        if token == Some(expected.as_str()) {
            return Some(AuthCtx {
                user_id: 0,
                username: "admin".into(),
                role: "owner".into(),
                device_id: String::new(),
            });
        }
    }
    // 轨① 会话 token。
    if let Some(t) = token {
        if let Ok((u, device_id)) = crate::collab::auth::check_session_with_device(t) {
            return Some(AuthCtx {
                user_id: u.id,
                username: u.username,
                role: u.role,
                device_id,
            });
        }
    }
    // 未命中就拒绝。零账号/无全局口令也绝不能合成 owner：否则攻击者可先调用
    // owner-only 的票据接口，再 redeem 绕过 bootstrap 初始化口令。
    None
}

/// header → AuthCtx。鉴权的同步 SQLite 查询挪进阻塞线程池:直接跑在 axum async worker
/// 上会钉住 reactor(会话短缓存命中时不落库,这层 spawn_blocking 兜的是未命中/首次)。
pub async fn auth_ctx(state: &CollabState, headers: &HeaderMap) -> Option<AuthCtx> {
    let auth_token = state.auth_token.clone();
    let token = bearer_of(headers);
    tokio::task::spawn_blocking(move || resolve_auth(&auth_token, token.as_deref()))
        .await
        .ok()
        .flatten()
}

/// 盘的入方向闸:**角色**或**设备契约**,两条路任一通过即可。
///
/// 为什么不能只留角色闸:设备码那条路进来的是 visitor,角色闸(owner 级)会把它一刀切死,
/// 「拿码就能连、默认只读」根本走不通。为什么不能只留契约闸:桌面自己、owner 令牌、
/// 老手机壳走的会话没有设备契约,只认契约会把它们全挡在外面。
///
/// 判定顺序:**先看契约**(它是用户在台账上亲手调的档,优先级最高,连 owner 角色也压得住 ——
/// 显式只读契约仍然有效),契约没话说才退回角色闸。
fn fs_allowed(ctx: &AuthCtx, need_write: bool) -> Result<(), String> {
    crate::collab::grants::fs_gate(&ctx.device_id, need_write)?;
    // 契约放行了,还要够角色 —— 除非它本来就是被契约放进来的那种设备。
    if role_rank(&ctx.role) >= 3 || has_grant(&ctx.device_id) {
        return Ok(());
    }
    Err("需要 owner 权限".into())
}

/// 这台设备有没有一行信任契约(有 = 它是经「设备码 / 同账号设备网」正式放进来的)。
fn has_grant(device_id: &str) -> bool {
    !device_id.trim().is_empty()
        && matches!(crate::collab::grants::get(device_id), Ok(Some(g)) if !g.revoked)
}

pub fn role_rank(role: &str) -> u8 {
    match role {
        "owner" => 3,
        "lead" | "collaborator" => 2,
        "visitor" => 1,
        _ => 0,
    }
}

// ───────────────────────── 小工具 ─────────────────────────

fn s_of(v: &Value, k: &str) -> String {
    v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string()
}

fn i_of(v: &Value, k: &str) -> Option<i64> {
    v.get(k).and_then(|x| x.as_i64())
}

fn forbid() -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(json!({"error":"需要 owner 权限"})),
    )
        .into_response()
}

pub fn err_resp(e: String) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": e })),
    )
        .into_response()
}

pub fn ok<T: Serialize>(t: T) -> Result<Value, String> {
    serde_json::to_value(t).map_err(|e| e.to_string())
}

/// spawn_blocking 结果 → HTTP 响应(业务错误 → 400 而非 500,便于前端展示)。
fn unwrap_api(out: Result<Result<Value, String>, tokio::task::JoinError>) -> Response {
    match out {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => (StatusCode::BAD_REQUEST, Json(json!({"error": e}))).into_response(),
        Err(e) => err_resp(format!("内部任务失败: {e}")),
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

mod source_ip;
use source_ip::client_ip;
mod login_routes;
use login_routes::*;
mod connect_routes;
use connect_routes::*;
mod session_routes;
use session_routes::*;
mod telemetry_routes;
use telemetry_routes::*;
mod email_routes;
use email_routes::*;
mod team_routes;
use team_routes::*;
mod mirror_routes;
use mirror_routes::*;
mod project_routes;
use project_routes::*;
mod task_chat_routes;
use task_chat_routes::*;
mod task_routes;
use task_routes::*;
mod lead_routes;
use lead_routes::*;
mod merge_routes;
use merge_routes::*;
mod tunnel_routes;
use tunnel_routes::*;
mod git_routes;
use git_routes::*;
mod websocket_routes;
use websocket_routes::*;
mod fs_read_routes;
use fs_read_routes::*;
mod peer_routes;
use peer_routes::*;
mod fs_write_routes;
use fs_write_routes::*;
mod router;
pub use project_routes::authority_only;
pub use merge_routes::project_repo_path;
pub use websocket_routes::ws_loop;
pub use router::collab_router;

#[cfg(test)]
mod tests;

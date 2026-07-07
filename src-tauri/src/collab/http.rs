//! 多人协作 HTTP 路由 —— 双壳共用(Docker server 壳 merge;桌面 hosting 内嵌)。
//! Task 2 会把 server.rs 里的全部 collab handler 迁进来;本文件先立地基:
//! CollabState / 分享码地址探测 / collab_router 骨架。

use crate::host::AppHandle;
use axum::Router;
use std::sync::Arc;

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
        if !ip.is_loopback() {
            out.push(format!("http://{ip}:{port}"));
        }
    }
    out
}

/// 协作路由。`with_ws=true` 时附带 /ws(桌面 hosting 用;server 壳自己有全量 /ws,传 false 防 merge 撞路由)。
/// Task 2 迁入全部 /api/collab/* 与 /git/* 路由;本任务先返回空 Router 保证双壳编译。
pub fn collab_router(state: CollabState, with_ws: bool) -> Router {
    let _ = with_ws; // Task 2 使用
    Router::new().with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advertise_env_override_wins() {
        std::env::set_var("POLARIS_ADVERTISE_URL", "http://nas.example:9000/, https://p.example");
        let urls = detect_advertise_urls(8484);
        std::env::remove_var("POLARIS_ADVERTISE_URL");
        assert_eq!(
            urls,
            vec![
                "http://nas.example:9000".to_string(),
                "https://p.example".to_string()
            ]
        );
    }

    #[test]
    fn advertise_autodetect_no_loopback() {
        std::env::remove_var("POLARIS_ADVERTISE_URL");
        for u in detect_advertise_urls(8484) {
            assert!(u.starts_with("http://"));
            assert!(!u.contains("127.0.0.1"));
            assert!(u.ends_with(":8484"));
        }
    }
}

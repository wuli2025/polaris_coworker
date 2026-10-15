//! Forwarding headers are usable only across an explicitly trusted TCP hop.
use axum::{extract::ConnectInfo, http::HeaderMap};
use std::net::SocketAddr;
use crate::collab::interconnect::{self, hardening::{normalize, Cidr}};

pub(super) fn client_ip(headers: &HeaderMap, peer: Option<ConnectInfo<SocketAddr>>) -> Result<String, String> {
    let networks = interconnect::load()?.ingress.networks()?;
    Ok(resolve(headers, peer.map(|p| p.0), &networks))
}

fn resolve(headers: &HeaderMap, peer: Option<SocketAddr>, trusted: &[Cidr]) -> String {
    let Some(peer) = peer else { return "unknown-peer".into(); };
    let peer = normalize(peer.ip());
    let is_trusted = |ip| trusted.iter().any(|network| network.contains(ip));
    if !is_trusted(peer) { return peer.to_string(); }
    // Prefer the verifiable chain over X-Real-IP; never fall back to another header
    // when a present XFF is malformed or contains only trusted hops.
    if headers.contains_key("x-forwarded-for") {
        let values: Vec<_> = headers.get_all("x-forwarded-for").iter().collect();
        for value in values.iter().rev() {
            let Ok(value) = value.to_str() else { return peer.to_string(); };
            for hop in value.rsplit(',') {
                let Ok(ip) = hop.trim().parse() else { return peer.to_string(); };
                let ip = normalize(ip);
                if !is_trusted(ip) { return ip.to_string(); }
            }
        }
        return peer.to_string();
    }
    let mut real = headers.get_all("x-real-ip").iter();
    let ip = real.next().and_then(|v| v.to_str().ok()).and_then(|v| v.trim().parse().ok());
    if real.next().is_some() { return peer.to_string(); }
    ip.map(normalize).map_or_else(|| peer.to_string(), |ip| ip.to_string())
}

#[cfg(test)]
#[path = "source_ip_tests.rs"]
mod tests;

//! Preserve the signed actor proof needed by Policy B at the destination host.
use axum::http::HeaderMap;
pub(super) fn forwarded(headers: &HeaderMap) -> Vec<(&'static str, String)> {
    ["authorization", "content-type", "accept", "x-polaris-device-assertion"].into_iter()
        .filter_map(|name| headers.get(name).and_then(|value| value.to_str().ok()).map(|value| (name, value.to_string())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn forwards_device_proof_but_not_client_proxy_trust_headers() -> Result<(), String> {
        let mut headers = HeaderMap::new();
        for (name, value) in [("authorization", "Bearer session"), ("x-polaris-device-assertion", "PA1.signed"),
            ("x-forwarded-for", "127.0.0.1"), ("host", "attacker.example")] {
            headers.insert(name, value.parse().map_err(|e| format!("{e}"))?);
        }
        assert_eq!(forwarded(&headers), vec![("authorization", "Bearer session".into()),
            ("x-polaris-device-assertion", "PA1.signed".into())]);
        Ok(())
    }
}

use std::collections::HashMap;

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// query map → `?k=v&...`(空则空串)。HTTP 与 WS 反代共用。
pub(super) fn build_qs(q: &HashMap<String, String>) -> String {
    if q.is_empty() {
        return String::new();
    }
    let enc: Vec<String> = q
        .iter()
        .map(|(k, v)| format!("{}={}", urlencode(k), urlencode(v)))
        .collect();
    format!("?{}", enc.join("&"))
}


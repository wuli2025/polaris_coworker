//! Identity sharing operations; public paths are re-exported by identity.
use super::*;

/// 分享码:`PLRS1-<base64url(json{c:裸码, a:[地址], u:账号中心, k:公钥指纹})>`。
/// 裸码仍单独入库,分享码只是传输壳——成员端解开后逐个探活地址、自动填主机、
/// 再用裸码入伙,零手填。
///
/// `u`/`k` 是联邦模式带的:主机的账号由云端账号中心统管时,收码人得知道
/// **去哪儿登账号**(u)以及**该信任哪把钥匙**(k,人眼核对防中间人)。
/// 本机账号模式下这两项为空,老客户端读不到也不影响(JSON 只加不改)。
pub fn encode_share_code(code: &str, addrs: &[String]) -> String {
    let (authority, kid) = share_authority_hint();
    let mut payload = serde_json::json!({ "c": code, "a": addrs });
    if !authority.is_empty() {
        payload["u"] = serde_json::json!(authority);
        payload["k"] = serde_json::json!(kid);
    }
    let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload.to_string());
    format!("PLRS1-{b64}")
}

/// 本机该往分享码里写哪个账号中心。
///  · 成员主机(delegated)→ 它委托的那个云端账号中心 + 已钉住的公钥指纹。
///  · 权威机自己(authority)→ 就写它自己的对外地址(POLARIS_ADVERTISE_URL)。
///  · 本机账号模式 → 空,分享码里不带这两项。
fn share_authority_hint() -> (String, String) {
    use crate::collab::authority::{self, Mode};
    match authority::mode() {
        Mode::Delegated(url) => {
            let kid = authority::pinned().map(|(_, _, k)| k).unwrap_or_default();
            (url, kid)
        }
        Mode::Authority => (
            std::env::var("POLARIS_ADVERTISE_URL").unwrap_or_default(),
            authority::kid().unwrap_or_default(),
        ),
        Mode::Local => (String::new(), String::new()),
    }
}

/// 分享码解出的内容。`authority` 空 = 对方是本机账号模式(老行为)。
pub struct ShareCode {
    pub code: String,
    pub addrs: Vec<String>,
    pub authority: String,
    pub kid: String,
}

/// 分享码 → 结构体。不是分享码/结构不符返回 None(裸码走旧流程)。
pub fn decode_share_code_full(s: &str) -> Option<ShareCode> {
    let b64 = s.trim().strip_prefix("PLRS1-")?;
    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(b64)
        .ok()?;
    let v: serde_json::Value = serde_json::from_slice(&raw).ok()?;
    Some(ShareCode {
        code: v.get("c")?.as_str()?.to_string(),
        addrs: v
            .get("a")?
            .as_array()?
            .iter()
            .filter_map(|x| x.as_str().map(String::from))
            .collect(),
        authority: v
            .get("u")
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string(),
        kid: v
            .get("k")
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string(),
    })
}

/// 分享码 → (裸码, 地址表)。老调用方沿用。
pub fn decode_share_code(s: &str) -> Option<(String, Vec<String>)> {
    decode_share_code_full(s).map(|v| (v.code, v.addrs))
}

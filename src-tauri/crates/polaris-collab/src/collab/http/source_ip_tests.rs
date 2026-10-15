use super::*;
use crate::collab::interconnect;

fn headers(real: &str, xff: &str) -> Result<HeaderMap, String> {
    let mut headers = HeaderMap::new();
    if !real.is_empty() { headers.insert("x-real-ip", real.parse().map_err(|e| format!("{e}"))?); }
    if !xff.is_empty() { headers.insert("x-forwarded-for", xff.parse().map_err(|e| format!("{e}"))?); }
    Ok(headers)
}

#[test]
fn direct_peer_ignores_forged_headers_and_missing_peer_has_a_bucket() -> Result<(), String> {
    let headers = headers("1.1.1.1", "2.2.2.2")?;
    assert_eq!(resolve(&headers, Some("192.0.2.8:8080".parse().map_err(|e| format!("{e}"))?), &[]), "192.0.2.8");
    assert_eq!(resolve(&headers, None, &[]), "unknown-peer");
    Ok(())
}

#[test]
fn trusted_proxy_walks_right_to_left_without_using_spoofed_leftmost() -> Result<(), String> {
    let trusted = [Cidr::parse("10.0.0.0/8")?, Cidr::parse("fd00::/8")?];
    let peer = Some("10.1.2.3:80".parse().map_err(|e| format!("{e}"))?);
    for (real, xff, expected) in [
        ("192.0.2.10", "", "192.0.2.10"),
        ("1.1.1.1", "192.0.2.10", "192.0.2.10"),
        ("1.1.1.1", "6.6.6.6, 192.0.2.10, 10.2.3.4, fd00::8", "192.0.2.10"),
        ("1.1.1.1", "6.6.6.6, invalid, 10.2.3.4", "10.1.2.3"),
        ("1.1.1.1", "10.2.3.4, fd00::8", "10.1.2.3"),
        ("1.1.1.1", "6.6.6.6, ::ffff:192.0.2.10", "192.0.2.10"),
    ] {
        assert_eq!(resolve(&headers(real, xff)?, peer, &trusted), expected);
    }
    let mut repeated = headers("", "6.6.6.6")?;
    repeated.append("x-forwarded-for", "192.0.2.10, 10.2.3.4".parse().map_err(|e| format!("{e}"))?);
    assert_eq!(resolve(&repeated, peer, &trusted), "192.0.2.10");
    let direct = Some("192.0.2.8:80".parse().map_err(|e| format!("{e}"))?);
    assert_eq!(resolve(&repeated, direct, &trusted), "192.0.2.8");
    Ok(())
}

#[test]
fn cidrs_and_policy_types_are_validated() -> Result<(), String> {
    for raw in ["10.0.0.1", "10.0.0.0/33", "::/129", "bad/8", "10.0.0.0/-1"] {
        assert!(Cidr::parse(raw).is_err());
        let json = serde_json::json!({"ingress":{"trustedProxyCidrs":[raw]}}).to_string();
        assert!(interconnect::resolve(Some(&json), |_| None).is_err());
    }
    assert!(Cidr::parse("::/0")?.contains("2001:db8::1".parse().map_err(|e| format!("{e}"))?));
    assert!(!Cidr::parse("10.0.0.0/8")?.contains("11.0.0.1".parse().map_err(|e| format!("{e}"))?));
    for raw in [r#"{"capacity":0}"#, r#"{"concurrency":65}"#, r#"{"capacity":1,"concurrency":2}"#,
        r#"{"timeoutSecs":301}"#, r#"{"timeoutSecs":"30"}"#, r#"{"timeoutSecs":1.5}"#] {
        assert!(interconnect::resolve(Some(&format!(r#"{{"mailDelivery":{raw}}}"#)), |_| None).is_err());
    }
    let defaults = interconnect::resolve(Some("{}"), |_| None)?;
    assert!(defaults.ingress.trusted_proxy_cidrs.is_empty());
    assert_eq!(defaults.mail_delivery.capacity, 32);
    Ok(())
}

use super::*;

#[test]
fn url_normalization_prefers_https_and_public_http_requires_consent() -> Result<(), String> {
    assert_eq!(norm_url(" example.com/ "), "https://example.com");
    assert_eq!(norm_url("https://a.com/"), "https://a.com");
    assert_eq!(norm_url("http://a.com"), "http://a.com");
    assert!(validate_authority_url("http://a.com", false)
        .err().is_some_and(|error| error.contains("明文 HTTP")));
    assert_eq!(
        validate_authority_url("http://a.com", true)?,
        "http://a.com"
    );
    assert_eq!(
        validate_authority_url("http://127.0.0.1:8080", false)?,
        "http://127.0.0.1:8080"
    );
    Ok(())
}

#[test]
fn device_code_parser_requires_the_complete_plr1_shape() -> Result<(), String> {
    let (code, node) = parse_device_code("plr1-abcd2345-node-identity")?;
    assert_eq!(code, "ABCD2345");
    assert_eq!(node, "node-identity");
    assert!(parse_device_code("ABCD2345").is_err());
    assert!(parse_device_code("PLR1-ABCD2345").is_err());
    assert!(parse_device_code("PLR1--node").is_err());
    assert!(parse_device_code("PLRK1-old-owner-token")
        .err().is_some_and(|error| error.starts_with("legacy_code_unsupported:")));
    Ok(())
}

#[test]
fn logged_out_status_never_exposes_a_stale_uid() {
    assert_eq!(current_uid(false, "uid-from-old-login".into()), "");
    assert_eq!(current_uid(true, "uid-current".into()), "uid-current");
}

#[test]
fn cached_token_without_a_live_tunnel_is_not_connected() {
    let link = Link {
        name: "stale".into(),
        port: 65_534,
        token: Some("cached-session".into()),
        phase: "ready".into(),
        error_code: String::new(),
        last_verified_at: now_secs(),
        err: String::new(),
    };
    assert!(
        !link_connected(&link),
        "刚验证过的 token 也不能把不存在的 tunnel 冒充成在线"
    );
}

/// sourceId 必须稳定且不超长(fsmount 拿它当 key,长 NodeId 截断后仍要唯一到实用)。
#[test]
fn source_id_is_stable_and_short() {
    let a = "abcdefghijklmnopqrstuvwxyz0123456789";
    assert_eq!(source_id(a), source_id(a));
    assert_eq!(source_id(a), "mesh-abcdefghijklmnop");
    assert_eq!(source_id("short"), "mesh-short");
}

/// 端口分配:新设备各得一口,已在册的沿用原口(重试不换口)。
#[test]
fn ports_are_unique_and_sticky() {
    links().lock().unwrap_or_else(|error| error.into_inner()).clear();
    let p1 = port_for("node-a");
    links().lock().unwrap_or_else(|error| error.into_inner()).insert(
        "node-a".into(),
        Link {
            name: "a".into(),
            port: p1,
            token: None,
            phase: "connecting".into(),
            error_code: String::new(),
            last_verified_at: 0,
            err: String::new(),
        },
    );
    let p2 = port_for("node-b");
    assert_ne!(p1, p2, "两台设备不能撞同一个本地口");
    assert_eq!(port_for("node-a"), p1, "已在册的必须沿用原端口");
    links().lock().unwrap_or_else(|error| error.into_inner()).clear();
}

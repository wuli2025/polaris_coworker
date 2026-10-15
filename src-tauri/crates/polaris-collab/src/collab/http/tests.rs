use super::*;

/// Range 解析:标准/开区间/后缀/越界钳制/看不懂回 None(整文件)。
#[test]
fn byte_range_parsing() {
    assert_eq!(parse_byte_range(Some("bytes=0-99"), 1000), Some((0, 99)));
    assert_eq!(parse_byte_range(Some("bytes=200-"), 1000), Some((200, 999)));
    assert_eq!(parse_byte_range(Some("bytes=-100"), 1000), Some((900, 999)));
    assert_eq!(
        parse_byte_range(Some("bytes=0-9999"), 1000),
        Some((0, 999)),
        "end 越界须钳到 len-1"
    );
    assert_eq!(
        parse_byte_range(Some("bytes=500-100"), 1000),
        None,
        "倒置区间无效,按整文件"
    );
    assert_eq!(
        parse_byte_range(Some("bytes=2000-"), 1000),
        None,
        "起点越界不可满足,按整文件"
    );
    assert_eq!(parse_byte_range(None, 1000), None);
    assert_eq!(parse_byte_range(Some("items=0-1"), 1000), None);
    assert_eq!(
        parse_byte_range(Some("bytes=-0"), 1000),
        None,
        "-0 无意义按整文件"
    );
    assert_eq!(
        parse_byte_range(Some("bytes=-100"), 0),
        None,
        "空文件后缀区间按整文件"
    );
    // 多区间只取第一段。
    assert_eq!(parse_byte_range(Some("bytes=0-1,5-9"), 1000), Some((0, 1)));
    assert_eq!(httpdate_from_secs(0), "Thu, 01 Jan 1970 00:00:00 GMT");
}

/// 已压缩格式不再二次压缩(白烧 CPU),其余一律可压。判不准不致命 —— zstd 对
/// 不可压数据是原样存储 —— 但这份名单直接决定小机器上的 CPU 账单。
#[test]
fn precompressed_ext_detection() {
    for p in [
        "a/b/photo.JPG",
        "x.zip",
        "dir/pkg.tar.gz",
        "v.mp4",
        "s.pdf",
        "app.exe",
    ] {
        assert!(is_precompressed(p), "{p} 应判为已压缩");
    }
    for p in [
        "notes.md",
        "src/main.rs",
        "a.txt",
        "data.json",
        "server.log",
        "无扩展名",
    ] {
        assert!(!is_precompressed(p), "{p} 应判为可压缩");
    }
    // 目录名里的点不能误当扩展名。
    assert!(!is_precompressed("v1.2.3/README"), "只看最后一段的扩展名");
}

/// 压缩级别的钳制与关闭开关。0 = 彻底不压(链路是千兆内网时该关)。
#[test]
fn zstd_level_clamped() {
    std::env::set_var("POLARIS_FS_ZSTD_LEVEL", "0");
    assert_eq!(zstd_level(), 0, "0 = 关闭压缩");
    std::env::set_var("POLARIS_FS_ZSTD_LEVEL", "99");
    assert_eq!(zstd_level(), 19, "超出上限须钳住,不能把 CPU 烧穿");
    std::env::set_var("POLARIS_FS_ZSTD_LEVEL", "不是数字");
    assert_eq!(zstd_level(), 3, "写错按默认,不因配置错误退化成不可用");
    std::env::remove_var("POLARIS_FS_ZSTD_LEVEL");
}

/// 自动探测与 env 覆写合在一个测试里跑:环境变量是进程级的,拆成两个测试
/// 会因并行执行互相污染(一个设着 POLARIS_ADVERTISE_URL,另一个恰好读到)。
#[test]
fn advertise_urls_autodetect_and_env_override() {
    std::env::remove_var("POLARIS_ADVERTISE_URL");
    for u in detect_advertise_urls(8484) {
        assert!(u.starts_with("http://"));
        assert!(!u.contains("127.0.0.1"));
        assert!(!u.contains("//198.18.") && !u.contains("//198.19."));
        assert!(u.ends_with(":8484"));
    }
    std::env::set_var(
        "POLARIS_ADVERTISE_URL",
        "http://nas.example:9000/, https://p.example",
    );
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
fn tun_fake_ip_range_detection() -> Result<(), std::net::AddrParseError> {
    let ip = |s: &str| s.parse::<std::net::IpAddr>();
    assert!(is_tun_fake_ip(&ip("198.18.0.1")?));
    assert!(is_tun_fake_ip(&ip("198.19.255.254")?));
    assert!(!is_tun_fake_ip(&ip("198.17.0.1")?));
    assert!(!is_tun_fake_ip(&ip("198.20.0.1")?));
    assert!(!is_tun_fake_ip(&ip("192.168.1.5")?));
    assert!(!is_tun_fake_ip(&ip("100.126.0.21")?));
    Ok(())
}

#[cfg(test)]
mod origin_gate_tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    /// 协作面 bootstrap 复用 `is_open()` 当闸(见 server.rs 的 from_fn 层)。
    /// 这里钉死它的两条语义：免口令且来源够格才为真；LAN_ONLY 下公网来源必须为假 ——
    /// 否则「只放行内网」的部署会被公网来客抢注第一个 owner 账号。
    #[test]
    fn is_open_是免口令与来源两个条件的与() {
        let h = HeaderMap::new();
        let lan = Some(SocketAddr::new(ip("192.168.1.5"), 5000));
        let wan = Some(SocketAddr::new(ip("8.8.8.8"), 5000));

        assert!(!origin_gate_with(false, false, lan, &h).is_open());
        assert!(!origin_gate_with(false, true, lan, &h).is_open());

        assert!(origin_gate_with(true, false, lan, &h).is_open());
        assert!(origin_gate_with(true, false, wan, &h).is_open());
        assert!(origin_gate_with(true, false, None, &h).is_open());

        assert!(origin_gate_with(true, true, lan, &h).is_open());
        assert!(!origin_gate_with(true, true, wan, &h).is_open());
        assert!(!origin_gate_with(true, true, None, &h).is_open());
    }

    #[test]
    fn 私网段判定() {
        for s in [
            "127.0.0.1",
            "10.0.0.1",
            "172.16.0.1",
            "172.31.255.254",
            "192.168.1.5",
            "169.254.1.1",
            "100.64.0.1",
            "100.78.103.101", // 实测的 Tailscale 地址
            "100.127.255.254",
        ] {
            assert!(is_private_ip(ip(s)), "{s} 应判为内网");
        }
        for s in [
            "8.8.8.8",
            "1.1.1.1",
            "172.32.0.1",  // 172.16/12 的上界外
            "100.63.0.1",  // 100.64/10 的下界外
            "100.128.0.1", // 100.64/10 的上界外
            "11.0.0.1",
        ] {
            assert!(!is_private_ip(ip(s)), "{s} 应判为公网");
        }
    }

    #[test]
    fn ipv6_与_v4_映射() {
        assert!(is_private_ip(ip("::1")));
        assert!(is_private_ip(ip("fc00::1")));
        assert!(is_private_ip(ip("fe80::1")));
        // v4-mapped 必须拆回 v4 判,否则内网 v4 经 v6 套接字进来会被误判成公网
        assert!(is_private_ip(ip("::ffff:192.168.1.1")));
        assert!(!is_private_ip(ip("::ffff:8.8.8.8")));
        assert!(!is_private_ip(ip("2001:db8::1")));
    }

    fn gate(
        enabled: bool,
        lan_only: bool,
        peer: Option<&str>,
        hdrs: &[(&str, &str)],
    ) -> OriginGate {
        let mut h = HeaderMap::new();
        for (k, v) in hdrs {
            h.insert(
                axum::http::HeaderName::from_bytes(k.as_bytes()).unwrap(),
                v.parse().unwrap(),
            );
        }
        let addr = peer.map(|s| SocketAddr::new(ip(s), 12345));
        origin_gate_with(enabled, lan_only, addr, &h)
    }

    #[test]
    fn 壳没开就一律不放行() {
        assert!(!gate(false, false, Some("192.168.1.5"), &[]).allows());
        assert!(!gate(false, true, Some("192.168.1.5"), &[]).allows());
    }

    #[test]
    fn 默认不分内外网一律放行() {
        // 现行默认:没设 POLARIS_AUTH_TOKEN = 谁都不用口令,来源不再是门槛。
        assert!(gate(true, false, Some("192.168.1.5"), &[]).allows());
        assert!(gate(true, false, Some("203.0.113.7"), &[]).allows());
        // 连「拿不到对端地址 / 反代后面」这些判不出来源的情况也照进,不再吃闭门羹。
        assert!(gate(true, false, None, &[]).allows());
        assert!(gate(
            true,
            false,
            Some("127.0.0.1"),
            &[("x-forwarded-for", "8.8.8.8")]
        )
        .allows());
    }

    #[test]
    fn lan_only_内网来源放行_公网来源不放行() {
        assert!(gate(true, true, Some("192.168.1.5"), &[]).allows());
        assert!(gate(true, true, Some("127.0.0.1"), &[]).allows());
        assert!(!gate(true, true, Some("203.0.113.7"), &[]).allows());
    }

    #[test]
    fn lan_only_拿不到对端地址时保守当公网() {
        assert!(!gate(true, true, None, &[]).allows());
    }

    #[test]
    fn lan_only_有反代且未声明可信时保守当公网() {
        // 反代场景对端恒是反代自己(127.0.0.1)。若按它判,全世界都成了内网。
        assert!(!gate(
            true,
            true,
            Some("127.0.0.1"),
            &[("x-forwarded-for", "8.8.8.8")]
        )
        .allows());
        assert!(!gate(true, true, Some("127.0.0.1"), &[("x-real-ip", "8.8.8.8")]).allows());
        // 连伪造成内网 IP 也不认(没声明可信就压根不读这个头)
        assert!(!gate(
            true,
            true,
            Some("127.0.0.1"),
            &[("x-forwarded-for", "192.168.1.5")]
        )
        .allows());
    }

    #[test]
    fn 鉴权分支() {
        // 免口令模式(机器没设 POLARIS_AUTH_TOKEN):默认 origin_ok 恒 true。
        let open = OriginGate {
            enabled: true,
            origin_ok: true,
        };
        // 只有 POLARIS_LAN_ONLY=1 且来源判成公网时才会是这个形状。
        let lan_only_wan = OriginGate {
            enabled: true,
            origin_ok: false,
        };

        // ① 没设口令 → 直接进,不需要任何凭据(NAS 用户的日常,也是这次改动的目的)
        let ctx = resolve_app_auth_token(&None, None, open).expect("免口令模式应放行");
        assert_eq!(ctx.role, "owner");
        assert_eq!(ctx.username, "local");

        // ② POLARIS_LAN_ONLY=1 + 公网来源 → 拒。这里绝不能掉进下面的「开放模式」兜底,
        //    否则那个开关等于没有。
        assert!(
            resolve_app_auth_token(&None, None, lan_only_wan).is_none(),
            "LAN_ONLY 下公网来源必须拒"
        );

        // ③ 显式设了口令 → 壳把 enabled 置 false → 任何来源都必须校验,
        //    否则等于把管理员刻意设的口令静默作废。
        let explicit = Some("my-secret".to_string());
        assert!(
            resolve_app_auth_token(&explicit, None, OriginGate::closed()).is_none(),
            "显式口令绝不能被来源绕过"
        );
        let ctx = resolve_app_auth_token(&explicit, Some("my-secret"), OriginGate::closed())
            .expect("口令对应放行");
        assert_eq!(ctx.username, "admin");

        // ④ 桌面 hosting 那种 enabled=false + 无口令 → 维持历史的全放行
        let ctx = resolve_app_auth_token(&None, None, OriginGate::closed()).expect("开放模式放行");
        assert_eq!(ctx.role, "owner");
    }

    #[cfg(not(feature = "desktop"))]
    #[test]
    fn docker_一键更新要求应用鉴权与窄接口配置() {
        assert!(!docker_update_auth_configured_with(None, false));
        assert!(!docker_update_auth_configured_with(Some("  "), false));
        assert!(docker_update_auth_configured_with(
            Some("machine-secret"),
            false
        ));
        assert!(docker_update_auth_configured_with(None, true));

        assert!(!updater_service_configured_with(None, Some("secret")));
        assert!(!updater_service_configured_with(
            Some("http://polaris-updater:8080/v1/update"),
            None
        ));
        assert!(!updater_service_configured_with(
            Some("http://polaris-updater:8080/metrics"),
            Some("secret")
        ));
        assert!(!updater_service_configured_with(
            Some("https://example.com/v1/update"),
            Some("secret")
        ));
        assert!(!updater_service_configured_with(
            Some("file:///v1/update"),
            Some("secret")
        ));
        assert!(updater_service_configured_with(
            Some("http://polaris-updater:8080/v1/update"),
            Some("secret")
        ));
    }

    #[cfg(not(feature = "desktop"))]
    #[test]
    fn docker_镜像_marker_优先于_watchtower_保留的旧环境变量() {
        assert_eq!(
            select_build_value(
                Some("target-rev".into()),
                Some("source-rev".into()),
                "local"
            ),
            "target-rev"
        );
        assert_eq!(
            select_build_value(None, Some("env-rev".into()), "local"),
            "env-rev"
        );
    }

    #[cfg(not(feature = "desktop"))]
    #[test]
    fn docker_明文_registry_只豁免精确的本机主机名() {
        for base in [
            "http://localhost:5000",
            "http://127.0.0.1:5000",
            "http://[::1]:5000",
            "http://registry:5000",
        ] {
            assert!(insecure_registry_host_is_local(base), "{base}");
        }
        for base in [
            "http://localhost.evil.example:5000",
            "http://127.0.0.1.evil.example:5000",
            "http://registry.evil.example:5000",
            "http://user@localhost:5000",
            "https://localhost:5000",
        ] {
            assert!(!insecure_registry_host_is_local(base), "{base}");
        }
    }

    #[cfg(not(feature = "desktop"))]
    #[test]
    fn docker_registry_同时接受多架构索引与单架构_manifest() {
        for media_type in [
            "application/vnd.oci.image.index.v1+json",
            "application/vnd.docker.distribution.manifest.list.v2+json",
            "application/vnd.oci.image.manifest.v1+json",
            "application/vnd.docker.distribution.manifest.v2+json",
        ] {
            assert!(REGISTRY_MANIFEST_ACCEPT.contains(media_type), "{media_type}");
        }
    }

    #[cfg(not(feature = "desktop"))]
    #[test]
    fn docker_镜像索引按当前架构选择且拒绝缺失平台() {
        let index = json!({
            "manifests": [
                {"digest":"sha256:arm", "platform":{"os":"linux", "architecture":"arm64"}},
                {"digest":"sha256:amd", "platform":{"os":"linux", "architecture":"amd64"}},
                {"digest":"sha256:win", "platform":{"os":"windows", "architecture":"amd64"}}
            ]
        });
        assert_eq!(
            select_platform_manifest(&index, "amd64").unwrap(),
            "sha256:amd"
        );
        assert_eq!(
            select_platform_manifest(&index, "arm64").unwrap(),
            "sha256:arm"
        );
        assert!(select_platform_manifest(&index, "riscv64").is_err());
        assert!(select_platform_manifest(&json!({"manifests":[]}), "amd64").is_err());
    }

    #[cfg(not(feature = "desktop"))]
    #[test]
    fn docker_镜像必须有可证明的_revision() {
        let config = json!({"config":{"Labels":{
            "org.opencontainers.image.revision":"abcdef123456",
            "io.polaris.app.version":"2.9.0"
        }}});
        assert_eq!(
            image_build_labels(&config).unwrap(),
            ("abcdef123456".into(), "2.9.0".into())
        );
        let no_version = json!({"config":{"Labels":{
            "org.opencontainers.image.revision":"abcdef123456"
        }}});
        assert_eq!(image_build_labels(&no_version).unwrap().1, "unknown");
        assert!(image_build_labels(&json!({"config":{"Labels":{}}})).is_err());
        assert!(image_build_labels(&json!({"config":{"Labels":{
            "org.opencontainers.image.revision":""
        }}}))
        .is_err());
    }

    #[cfg(not(feature = "desktop"))]
    #[test]
    fn docker_更新状态只在新_boot_命中目标_revision_时成功() {
        let request = json!({
            "sourceBootId":"old-boot",
            "targetRevision":"target-rev",
            "deadline":200
        });
        let old = json!({"bootId":"old-boot", "buildRevision":"old-rev"});
        let wrong = json!({"bootId":"new-boot", "buildRevision":"other-rev"});
        let target = json!({"bootId":"new-boot", "buildRevision":"target-rev"});

        assert_eq!(
            derive_docker_update_state(&request, None, &old, 100),
            "triggering"
        );
        assert_eq!(
            derive_docker_update_state(
                &request,
                Some(&json!({"state":"watchtower_returned"})),
                &old,
                100,
            ),
            "waiting_restart"
        );
        assert_eq!(
            derive_docker_update_state(&request, None, &wrong, 200),
            "unconfirmed"
        );
        assert_eq!(
            derive_docker_update_state(&request, None, &target, 250),
            "succeeded"
        );
        assert_eq!(
            derive_docker_update_state(&request, Some(&json!({"state":"failed"})), &old, 100,),
            "failed"
        );
    }

    #[cfg(not(feature = "desktop"))]
    #[test]
    fn docker_更新_request_id_不能逃逸状态目录() {
        assert!(valid_update_request_id("1723456789-0"));
        assert!(!valid_update_request_id(""));
        assert!(!valid_update_request_id("../request"));
        assert!(!valid_update_request_id("123.json"));
        assert!(!valid_update_request_id(&"1".repeat(65)));
    }

    #[test]
    fn 落盘的老口令不再生效() {
        // 网页向导设过的口令曾经写在 collab.db,忘了就没有找回入口 —— 现在 effective_token
        // 只认环境变量,老库里那条即使还在也彻底失效,升级上来的用户不会被卡在门外。
        assert!(effective_token(&None).is_none());
        assert_eq!(
            effective_token(&Some("env-token".into())).as_deref(),
            Some("env-token")
        );
    }
}

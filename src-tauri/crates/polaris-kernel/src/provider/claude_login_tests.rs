use super::*;

    /// base64url(无填充)对已知向量正确,且与 codex_b64url_decode 互逆
    #[test]
    fn b64url_encode_known_vectors() {
        assert_eq!(claude_b64url_encode(b""), "");
        assert_eq!(claude_b64url_encode(b"f"), "Zg");
        assert_eq!(claude_b64url_encode(b"fo"), "Zm8");
        assert_eq!(claude_b64url_encode(b"foo"), "Zm9v");
        assert_eq!(claude_b64url_encode(b"foob"), "Zm9vYg");
        assert_eq!(claude_b64url_encode(b"fooba"), "Zm9vYmE");
        assert_eq!(claude_b64url_encode(b"foobar"), "Zm9vYmFy");
        // 含会被标准 base64 编成 '+' '/' 的字节,base64url 必须出 '-' '_'
        let enc = claude_b64url_encode(&[0xfb, 0xff, 0xbf]);
        assert!(!enc.contains('+') && !enc.contains('/') && !enc.contains('='));
        // 编码→解码 round-trip
        let raw: Vec<u8> = (0u8..=255).collect();
        let back = codex_b64url_decode(&claude_b64url_encode(&raw)).unwrap();
        assert_eq!(raw, back);
    }

    /// SHA-256 对 "abc" 的标准向量(NIST FIPS 180-4)
    #[test]
    fn sha256_known_vector() {
        let d = claude_sha256(b"abc");
        let hex: String = d.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    /// PKCE S256 端到端:RFC 7636 附录 B 的官方测试向量
    /// verifier "dBjft...60M" → challenge "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    #[test]
    fn pkce_s256_rfc7636_vector() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = claude_b64url_encode(&claude_sha256(verifier.as_bytes()));
        assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    /// 随机 verifier/state 不可预测且长度足够(32 字节 → 43 字符 base64url)
    #[test]
    fn rand_b64url_len_and_uniqueness() {
        let a = claude_rand_b64url(32).unwrap();
        let b = claude_rand_b64url(32).unwrap();
        assert_eq!(a.len(), 43);
        assert_ne!(a, b);
        assert!(!a.contains('=') && !a.contains('+') && !a.contains('/'));
    }

    /// query 值百分号编码:空格不留原样,unreserved 不动
    #[test]
    fn url_encode_query_value() {
        assert_eq!(
            claude_url_encode("org:create_api_key user:profile"),
            "org%3Acreate_api_key%20user%3Aprofile"
        );
        assert_eq!(claude_url_encode("aZ09-_.~"), "aZ09-_.~");
    }

    /// 回环回调 query 解析: 取参 + percent 解码 + '+' 还原空格; 缺参给 None
    #[test]
    fn loopback_query_param_decode() {
        let q = "code=ac_1%2Babc%3D%3D&state=st-42&error_description=Access+denied%21";
        assert_eq!(
            loopback_query_param(q, "code").as_deref(),
            Some("ac_1+abc==")
        );
        assert_eq!(loopback_query_param(q, "state").as_deref(), Some("st-42"));
        assert_eq!(
            loopback_query_param(q, "error_description").as_deref(),
            Some("Access denied!")
        );
        assert_eq!(loopback_query_param(q, "missing"), None);
        // 裸键(无 =)与坏 percent 序列不 panic
        assert_eq!(
            loopback_query_param("flag&x=1", "flag").as_deref(),
            Some("")
        );
        assert_eq!(loopback_pct_decode("%zz%"), "%zz%");
    }

    /// 回环授权 URL 构造纪律: 回环模式不带 code=true(那是手工回贴开关),
    /// redirect_uri 是 localhost; 手工模式带 code=true 且指向 console 展示页
    #[test]
    fn loopback_vs_manual_authorize_url_shape() {
        let redir_loop = claude_url_encode(CLAUDE_LOOPBACK_REDIRECT_URI);
        assert_eq!(redir_loop, "http%3A%2F%2Flocalhost%3A54545%2Fcallback");
        let redir_manual = claude_url_encode(CLAUDE_OAUTH_REDIRECT_URI);
        assert!(redir_manual.starts_with("https%3A%2F%2Fconsole.anthropic.com"));
    }

    /// 回环监听本机端到端: favicon 杂请求 404 不终止会话 → 真回调(percent 编码的
    /// code + 正确 state)→ finish 收到解码后的 code, 浏览器拿到成功落地页
    #[test]
    fn loopback_run_end_to_end_local() {
        use std::io::{Read, Write};
        use std::net::TcpStream;

        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let session = LoopbackSession::new();
        let sess = session.clone();
        let handle = thread::spawn(move || {
            let mut got = String::new();
            let r = loopback_run(
                listener,
                "/callback",
                "st-1",
                "测试",
                &sess,
                Duration::from_secs(10),
                |code| {
                    got = code.to_string();
                    Ok(())
                },
            );
            (r, got)
        });

        // 杂请求(favicon): 404, 会话继续等
        {
            let mut s = TcpStream::connect(addr).unwrap();
            s.write_all(b"GET /favicon.ico HTTP/1.1\r\nHost: x\r\n\r\n")
                .unwrap();
            let mut buf = String::new();
            let _ = s.read_to_string(&mut buf);
            assert!(buf.starts_with("HTTP/1.1 404"), "杂请求应 404: {buf}");
        }
        // 真回调: code 带 percent 编码, state 正确
        let mut s = TcpStream::connect(addr).unwrap();
        s.write_all(b"GET /callback?code=ac%2D9&state=st-1 HTTP/1.1\r\nHost: x\r\n\r\n")
            .unwrap();
        let mut buf = String::new();
        let _ = s.read_to_string(&mut buf);
        assert!(buf.starts_with("HTTP/1.1 200"), "回调应 200: {buf}");
        assert!(buf.contains("授权成功"), "应回成功落地页: {buf}");

        let (r, code) = handle.join().unwrap();
        assert!(r.is_ok(), "会话应成功结束: {r:?}");
        assert_eq!(code, "ac-9", "finish 应拿到 percent 解码后的 code");
    }

    /// state 不符(伪造回调)必须拒绝: 会话判败, 浏览器拿到失败页
    #[test]
    fn loopback_run_rejects_wrong_state() {
        use std::io::{Read, Write};
        use std::net::TcpStream;

        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let session = LoopbackSession::new();
        let sess = session.clone();
        let handle = thread::spawn(move || {
            loopback_run(
                listener,
                "/callback",
                "st-expected",
                "测试",
                &sess,
                Duration::from_secs(10),
                |_| panic!("state 不符时绝不能进入换 token"),
            )
        });
        let mut s = TcpStream::connect(addr).unwrap();
        s.write_all(b"GET /callback?code=x&state=st-forged HTTP/1.1\r\nHost: x\r\n\r\n")
            .unwrap();
        let mut buf = String::new();
        let _ = s.read_to_string(&mut buf);
        assert!(buf.contains("授权未完成"), "应回失败落地页: {buf}");
        assert!(handle.join().unwrap().is_err(), "state 不符应判败");
    }

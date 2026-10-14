#[cfg(all(test, feature = "server", not(feature = "desktop")))]
mod exec_gate_tests {
    use super::*;

    /// 起一个只挂 api_router 的真服务,返回端口与停机开关。
    async fn serve(auth_token: Option<String>) -> (u16, tokio::sync::oneshot::Sender<()>) {
        let (tx, _rx) = broadcast::channel(64);
        let state = ApiState {
            app: AppHandle::new(tx.clone()),
            tx,
            auth_token: Arc::new(auth_token),
            // 这组测试盯的是 exec 的 fail-closed:不论壳开不开免口令,没真凭据都得拒。
            // 置 false 走的是「桌面 hosting / 开放模式」那条兜底,更严格地覆盖到分支。
            open_no_auth: false,
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (stx, srx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            let _ = axum::serve(listener, api_router(state))
                .with_graceful_shutdown(async {
                    let _ = srx.await;
                })
                .await;
        });
        (port, stx)
    }

    /// 发一条 exec 请求,返回 (状态码, 响应体)。
    async fn post_exec(port: u16, token: Option<&str>, body: Value) -> (u16, String) {
        let token = token.map(|s| s.to_string());
        tokio::task::spawn_blocking(move || {
            let mut r = ureq::post(&format!("http://127.0.0.1:{port}/api/exec"));
            if let Some(t) = token {
                r = r.set("Authorization", &format!("Bearer {t}"));
            }
            match r.send_json(body) {
                Ok(resp) => (resp.status(), resp.into_string().unwrap_or_default()),
                Err(ureq::Error::Status(code, resp)) => {
                    (code, resp.into_string().unwrap_or_default())
                }
                Err(e) => panic!("请求失败: {e}"),
            }
        })
        .await
        .unwrap()
    }

    fn tmp_db(name: &str) -> std::path::PathBuf {
        let tmp = std::env::temp_dir().join(format!("polaris-execgate-{name}.db"));
        let _ = std::fs::remove_file(&tmp);
        std::env::set_var("POLARIS_COLLAB_DB", &tmp);
        tmp
    }

    /// 最关键的一条:没设访问口令时,基础面会「合成 owner 全放行」(开放模式)。
    /// 那对读接口尚可,对 exec 等于把无鉴权 shell 挂在隧道上。必须 fail-closed,
    /// **且不受远程执行总开关影响** —— 哪怕开关开着也得拒。
    #[tokio::test(flavor = "multi_thread")]
    async fn open_mode_is_fail_closed_even_when_enabled() {
        let _g = crate::collab::db::TEST_LOCK.lock().unwrap();
        let tmp = tmp_db("open");
        std::env::remove_var("POLARIS_REQUIRE_LOGIN");
        crate::exec::set_policy(true, None).unwrap(); // 开关开着,也必须拒

        let (port, stx) = serve(None).await; // None = 未设访问口令 = 开放模式
        let (code, body) = post_exec(port, None, json!({"cmd":"cargo","args":["--version"]})).await;
        assert_eq!(code, 403, "开放模式必须拒绝 exec,实际 body={body}");
        assert!(body.contains("未设访问口令"), "body={body}");

        // 同一台服务上,/api/invoke 在开放模式下仍照旧放行 —— 证明这条 fail-closed
        // 是 exec 专属加严,没有殃及既有的基础面语义。
        let ok = tokio::task::spawn_blocking(move || {
            ureq::post(&format!("http://127.0.0.1:{port}/api/invoke"))
                .send_json(json!({"cmd":"kb_root","args":{}}))
                .map(|r| r.status())
        })
        .await
        .unwrap();
        assert_eq!(ok.ok(), Some(200), "开放模式下 /api/invoke 应维持原行为");

        let _ = stx.send(());
        std::env::remove_var("POLARIS_COLLAB_DB");
        let _ = std::fs::remove_file(&tmp);
    }

    /// 设了访问口令后:不带/带错 token = 401;带对 token 但总开关没开 = 403。
    #[tokio::test(flavor = "multi_thread")]
    async fn token_required_then_switch_required() {
        let _g = crate::collab::db::TEST_LOCK.lock().unwrap();
        let tmp = tmp_db("token");
        std::env::remove_var("POLARIS_REQUIRE_LOGIN");
        crate::exec::set_policy(false, None).unwrap(); // 总开关关

        let (port, stx) = serve(Some("s3cret".into())).await;
        let body = json!({"cmd":"cargo","args":["--version"]});

        let (code, _) = post_exec(port, None, body.clone()).await;
        assert_eq!(code, 401, "无 token 必须 401");
        let (code, _) = post_exec(port, Some("wrong"), body.clone()).await;
        assert_eq!(code, 401, "错 token 必须 401");

        let (code, b) = post_exec(port, Some("s3cret"), body.clone()).await;
        assert_eq!(code, 403, "口令对但总开关没开,必须 403");
        assert!(b.contains("未开启远程执行"), "body={b}");

        let _ = stx.send(());
        std::env::remove_var("POLARIS_COLLAB_DB");
        let _ = std::fs::remove_file(&tmp);
    }

    /// 四道闸全过:真起进程、真拿到 stdout 与退出码;白名单外的命令仍是 403。
    #[tokio::test(flavor = "multi_thread")]
    async fn full_pass_runs_and_still_enforces_whitelist() {
        let _g = crate::collab::db::TEST_LOCK.lock().unwrap();
        let tmp = tmp_db("pass");
        std::env::remove_var("POLARIS_REQUIRE_LOGIN");
        crate::exec::set_policy(true, None).unwrap();

        let (port, stx) = serve(Some("s3cret".into())).await;

        let (code, b) = post_exec(
            port,
            Some("s3cret"),
            json!({"cmd":"cargo","args":["--version"]}),
        )
        .await;
        assert_eq!(code, 200, "闸门全过应 200,body={b}");
        let v: Value = serde_json::from_str(&b).unwrap();
        assert_eq!(v["ok"], true, "body={b}");
        assert_eq!(v["exit_code"], 0);
        assert_eq!(v["mode"], "whitelist");
        assert!(
            v["stdout"].as_str().unwrap_or("").contains("cargo"),
            "应拿到真实 stdout,body={b}"
        );

        // 白名单闸仍在:鉴权过了不等于什么都能跑。
        let (code, b) =
            post_exec(port, Some("s3cret"), json!({"cmd":"rm","args":["-rf","/"]})).await;
        assert_eq!(code, 403, "白名单外命令必须 403,body={b}");

        let _ = stx.send(());
        std::env::remove_var("POLARIS_COLLAB_DB");
        let _ = std::fs::remove_file(&tmp);
    }

    // ── /api/file 的白名单闸(手机端「远程预览」的读口)────────────────────────
    //
    // 起真服务发真 GET。两条都是踩过的坑:
    //  ① 项目绑了工作目录后,产物落在用户自选目录里,白名单没跟上 → 手机端预览一律
    //     403「路径不在允许范围」(功能加了、读口没跟上);
    //  ② 放开工作目录不等于开放整机:目录外的文件必须照旧 403。
    #[tokio::test(flavor = "multi_thread")]
    async fn file_whitelist_covers_project_work_dir_only() {
        let _g = crate::collab::db::TEST_LOCK.lock().unwrap();
        let tmp = tmp_db("file");
        std::env::remove_var("POLARIS_REQUIRE_LOGIN");

        // 工作目录(白名单内)与它外面的一个目录(白名单外)
        let root = std::env::temp_dir().join("polaris-filegate");
        let inside = root.join("work");
        let outside = root.join("elsewhere");
        std::fs::create_dir_all(&inside).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let art = inside.join("dashboard.html");
        std::fs::write(&art, "<h1>看板</h1>").unwrap();
        let secret = outside.join("secret.txt");
        std::fs::write(&secret, "不该被读到").unwrap();

        // 建项目并绑工作目录(测试里 conv 没 init,STATE_PATH 为空 → persist 是空操作,
        // 不会污染真实 state.json)
        let p = crate::conv::conv_create_project("预览白名单测试".into()).unwrap();
        crate::conv::conv_set_project_work_dir(
            p.id.clone(),
            Some(inside.to_string_lossy().to_string()),
        )
        .unwrap();

        let (port, stx) = serve(Some("s3cret".into())).await;
        let get = |path: std::path::PathBuf| async move {
            tokio::task::spawn_blocking(move || {
                // pct_encode 只留 unreserved,Windows 路径的 `:` `\` 都会转义,当查询值够用
                let url = format!(
                    "http://127.0.0.1:{port}/api/file?path={}",
                    pct_encode(&path.to_string_lossy())
                );
                match ureq::get(&url).set("Authorization", "Bearer s3cret").call() {
                    Ok(r) => (r.status(), r.into_string().unwrap_or_default()),
                    Err(ureq::Error::Status(c, r)) => (c, r.into_string().unwrap_or_default()),
                    Err(e) => panic!("请求失败: {e}"),
                }
            })
            .await
            .unwrap()
        };

        let (code, body) = get(art.clone()).await;
        assert_eq!(code, 200, "工作目录内的产物必须能读,body={body}");
        assert!(body.contains("看板"), "应拿到真实文件内容,body={body}");

        let (code, body) = get(secret.clone()).await;
        assert_eq!(code, 403, "工作目录外的文件必须照旧拒绝,body={body}");
        assert!(body.contains("不在允许范围"), "body={body}");

        // ── 相对路径:手机端点开「对话正文里提到的文件」拿到的往往是助手随手写的
        //    相对路径(`报告/周报.md`),按进程 CWD 解析必落空 → 得逐个白名单根去拼。
        let get_s = |raw: String| async move {
            tokio::task::spawn_blocking(move || {
                let url = format!("http://127.0.0.1:{port}/api/file?path={}", pct_encode(&raw));
                match ureq::get(&url).set("Authorization", "Bearer s3cret").call() {
                    Ok(r) => (r.status(), r.into_string().unwrap_or_default()),
                    Err(ureq::Error::Status(c, r)) => (c, r.into_string().unwrap_or_default()),
                    Err(e) => panic!("请求失败: {e}"),
                }
            })
            .await
            .unwrap()
        };
        std::fs::create_dir_all(inside.join("报告")).unwrap();
        std::fs::write(inside.join("报告/周报.md"), "# 本周进展").unwrap();

        let (code, body) = get_s("报告/周报.md".to_string()).await;
        assert_eq!(code, 200, "相对路径应能在白名单根下解析到,body={body}");
        assert!(body.contains("本周进展"), "body={body}");

        let (code, body) = get_s("./dashboard.html".to_string()).await;
        assert_eq!(code, 200, "`./` 前缀也要认,body={body}");
        assert!(body.contains("看板"), "body={body}");

        // 相对路径不等于可以往上爬:`..` 穿越到白名单外仍旧拒绝(canonicalize 后复核白名单)。
        let (code, body) = get_s("../elsewhere/secret.txt".to_string()).await;
        assert_ne!(code, 200, "相对路径 `..` 穿越必须挡住,body={body}");

        let (code, body) = get_s("不存在的文件.md".to_string()).await;
        assert_eq!(
            code, 404,
            "解析不到的相对路径应报不存在而非权限,body={body}"
        );

        let _ = stx.send(());
        std::env::remove_var("POLARIS_COLLAB_DB");
        let _ = std::fs::remove_file(&tmp);
        let _ = std::fs::remove_dir_all(&root);
    }
}

// ── 手机端「说一句就进电脑上的项目」的数据面链路 ────────────────────────────
//
// 盯的是一条真踩过的坑:手机原先拿本地生成的 `m-<时间戳>` 当 conversationId,主机不认识
// 就把它挂到「第一个未归档项目」下(conv::ensure_writable_or_create),而 claude 的 cwd 是
// 按 conversation→project→work_dir 解析的 —— 于是用户在手机上选哪个项目根本不起作用,
// 活干在了另一个目录里。修法是:选了项目就走 conv_create_conversation 拿主机发的真会话 id。
// 这里起真服务、发真 /api/invoke,把手机那套调用顺序原样走一遍,并一路验到 cwd 的落点。

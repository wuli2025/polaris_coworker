#[cfg(all(test, feature = "server", not(feature = "desktop")))]
mod open_access_e2e_tests {
    use super::*;

    /// 起真服务。connect_info=true 时按 server.rs 的方式挂 ConnectInfo。
    async fn serve(
        auth_token: Option<String>,
        open_no_auth: bool,
        connect_info: bool,
    ) -> (u16, tokio::sync::oneshot::Sender<()>) {
        let (tx, _rx) = broadcast::channel(64);
        let state = ApiState {
            app: AppHandle::new(tx.clone()),
            tx,
            auth_token: Arc::new(auth_token),
            open_no_auth,
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (stx, srx) = tokio::sync::oneshot::channel::<()>();
        let router = api_router(state);
        tokio::spawn(async move {
            let shutdown = async {
                let _ = srx.await;
            };
            if connect_info {
                let _ = axum::serve(
                    listener,
                    router.into_make_service_with_connect_info::<SocketAddr>(),
                )
                .with_graceful_shutdown(shutdown)
                .await;
            } else {
                let _ = axum::serve(listener, router)
                    .with_graceful_shutdown(shutdown)
                    .await;
            }
        });
        (port, stx)
    }

    /// 不带任何凭据打一发 /api/invoke,返回状态码。
    /// 用一条不存在的命令:鉴权过了会走到分发并得 404「未知命令」,鉴权没过则是 401。
    /// 401 vs 404 正好把「鉴权层」和「分发层」分开,不必依赖任何真命令与数据库。
    async fn probe(port: u16) -> u16 {
        tokio::task::spawn_blocking(move || {
            match ureq::post(&format!("http://127.0.0.1:{port}/api/invoke"))
                .send_json(json!({"cmd":"__auth_probe","args":{}}))
            {
                Ok(r) => r.status(),
                Err(ureq::Error::Status(c, _)) => c,
                Err(e) => panic!("请求失败: {e}"),
            }
        })
        .await
        .unwrap()
    }

    /// 没设 POLARIS_AUTH_TOKEN → 直接进,不能 401。
    #[tokio::test]
    async fn 没设口令时直接进得去() {
        let (port, stop) = serve(None, true, true).await;
        let code = probe(port).await;
        let _ = stop.send(());
        assert_ne!(code, 401, "免口令模式不该被任何口令框挡住");
        assert_eq!(code, 404, "应已过鉴权、走到分发拿到「未知命令」");
    }

    /// 管理员显式设了口令 → 一律校验,免口令那条路不生效。
    #[tokio::test]
    async fn 显式口令不被绕过() {
        let (port, stop) = serve(Some("my-secret".into()), true, true).await;
        let code = probe(port).await;
        let _ = stop.send(());
        assert_eq!(code, 401, "显式设的口令不能被静默作废");
    }

    /// 拿不到对端地址(反代后面 / 没挂 ConnectInfo)也照进 —— 这条以前是 401,
    /// 群晖反代、隧道转发过来的用户就卡在这儿,现在不再拦。
    #[tokio::test]
    async fn 拿不到对端地址也进得去() {
        let (port, stop) = serve(None, true, false).await;
        let code = probe(port).await;
        let _ = stop.send(());
        assert_eq!(code, 404, "免口令模式下不看来源,应已过鉴权走到分发");
    }

    /// /api/auth/state:免口令机器要明确告诉前端「别弹框」(can_setup=false + open_to_me=true),
    /// 否则老前端会照旧弹「设置访问口令」向导,又造出一个没人记得住的口令。
    #[tokio::test]
    async fn 未设口令时前端不该弹任何框() {
        let (port, stop) = serve(None, true, true).await;
        let body = tokio::task::spawn_blocking(move || {
            ureq::get(&format!("http://127.0.0.1:{port}/api/auth/state"))
                .call()
                .unwrap()
                .into_string()
                .unwrap()
        })
        .await
        .unwrap();
        let _ = stop.send(());
        let v: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["initialized"], false);
        assert_eq!(v["can_setup"], false, "网页端设口令的向导已停用");
        assert_eq!(v["open_to_me"], true);
    }

    /// 网页端设口令的入口已停用:不论机器有没有口令,一律 410,不再往库里写任何东西。
    #[tokio::test]
    async fn 网页端不再能设口令() {
        for token in [None, Some("env-token".to_string())] {
            let (port, stop) = serve(token, true, true).await;
            let code = tokio::task::spawn_blocking(move || {
                match ureq::post(&format!("http://127.0.0.1:{port}/api/auth/setup"))
                    .send_json(json!({"token":"whatever"}))
                {
                    Ok(r) => r.status(),
                    Err(ureq::Error::Status(c, _)) => c,
                    Err(e) => panic!("请求失败: {e}"),
                }
            })
            .await
            .unwrap();
            let _ = stop.send(());
            assert_eq!(code, 410, "首次访问向导已停用,应直接告知功能没了");
        }
    }
}

// ── /api/exec 的鉴权闸(HTTP 层真起服务测) ──────────────────────────────────
//
// exec 是本仓唯一「外部可触发本机任意进程」的入口,它的边界不能只靠编译期保证。
// 这组测试起真 axum 服务、发真 HTTP 请求,逐条验证闸门。
// 只在 server flavor 跑:桌面 flavor 的 AppHandle 是 tauri 真句柄,测试里造不出。

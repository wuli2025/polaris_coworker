#[cfg(all(test, feature = "server", not(feature = "desktop")))]
mod mobile_project_e2e_tests {
    use super::*;

    async fn serve(auth_token: Option<String>) -> (u16, tokio::sync::oneshot::Sender<()>) {
        let (tx, _rx) = broadcast::channel(64);
        let state = ApiState {
            app: AppHandle::new(tx.clone()),
            tx,
            auth_token: Arc::new(auth_token),
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

    /// 学手机端 net.ts 的 invoke():POST /api/invoke {cmd,args} + Bearer。返回 (状态码, 响应体)。
    async fn invoke(port: u16, cmd: &str, args: Value) -> (u16, Value) {
        let cmd = cmd.to_string();
        tokio::task::spawn_blocking(move || {
            let sent = ureq::post(&format!("http://127.0.0.1:{port}/api/invoke"))
                .set("Authorization", "Bearer s3cret")
                .send_json(json!({"cmd": cmd, "args": args}));
            let (code, text) = match sent {
                Ok(r) => (r.status(), r.into_string().unwrap_or_default()),
                Err(ureq::Error::Status(c, r)) => (c, r.into_string().unwrap_or_default()),
                Err(e) => panic!("请求失败: {e}"),
            };
            (
                code,
                serde_json::from_str::<Value>(&text).unwrap_or(Value::Null),
            )
        })
        .await
        .unwrap()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn phone_picks_project_then_cwd_follows_it() {
        let _g = crate::collab::db::TEST_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("polaris-mobileproj.db");
        let _ = std::fs::remove_file(&tmp);
        std::env::set_var("POLARIS_COLLAB_DB", &tmp);
        std::env::remove_var("POLARIS_REQUIRE_LOGIN");

        // 电脑上两个真实存在的文件夹(用户的两个大项目仓库)
        let root = std::env::temp_dir().join("polaris-mobileproj");
        let alpha = root.join("alpha-repo");
        let beta = root.join("beta-repo");
        std::fs::create_dir_all(&alpha).unwrap();
        std::fs::create_dir_all(&beta).unwrap();

        let (port, stx) = serve(Some("s3cret".into())).await;

        // ① 手机在「电脑项目」页建项目并绑文件夹(WorkScreen 的新建 + 绑定工作目录)
        let (c, pa) = invoke(port, "conv_create_project", json!({"name":"阿尔法"})).await;
        assert_eq!(c, 200, "建项目应成功,body={pa}");
        let pa_id = pa["id"].as_str().expect("项目应有 id").to_string();
        let (c, pb) = invoke(port, "conv_create_project", json!({"name":"贝塔"})).await;
        assert_eq!(c, 200, "建项目应成功,body={pb}");
        let pb_id = pb["id"].as_str().unwrap().to_string();

        for (pid, dir) in [(&pa_id, &alpha), (&pb_id, &beta)] {
            let (c, b) = invoke(
                port,
                "conv_set_project_work_dir",
                json!({"projectId": pid, "workDir": dir.to_string_lossy()}),
            )
            .await;
            assert_eq!(c, 200, "绑定工作目录应成功,body={b}");
        }

        // 手输路径打错是手机上最常见的失手 —— 必须当场报错,不能默默存下一个无效 cwd
        let (c, b) = invoke(
            port,
            "conv_set_project_work_dir",
            json!({"projectId": pa_id, "workDir": root.join("并不存在").to_string_lossy()}),
        )
        .await;
        assert_ne!(c, 200, "不存在的目录必须拒绝,body={b}");
        assert_eq!(
            crate::conv::project_work_dir(&pa_id).as_deref(),
            Some(alpha.to_string_lossy().as_ref()),
            "被拒的绑定不能把原来那个有效目录改掉"
        );

        // ② 手机拉项目清单(说「打开××项目」时就是在这份清单里匹配名字/文件夹名)
        let (c, list) = invoke(port, "conv_list_projects", json!({})).await;
        assert_eq!(c, 200);
        let arr = list.as_array().expect("应返回数组");
        let found = arr
            .iter()
            .find(|p| p["id"].as_str() == Some(pa_id.as_str()))
            .expect("清单里应有刚建的项目");
        assert_eq!(
            found["work_dir"].as_str(),
            Some(alpha.to_string_lossy().as_ref()),
            "清单必须带 work_dir —— 手机端顶栏显示、按文件夹名匹配都靠它"
        );

        // ③ 选中阿尔法后开新对话:走 conv_create_conversation(projectId),拿主机发的真 id
        let (c, conv) = invoke(
            port,
            "conv_create_conversation",
            json!({"projectId": pa_id}),
        )
        .await;
        assert_eq!(c, 200, "在项目下建会话应成功,body={conv}");
        let cid = conv["id"].as_str().expect("会话应有 id").to_string();

        // ④ 这一跳正是 chat 管线解析 cwd 的路径:conversation → project → work_dir
        assert_eq!(
            crate::conv::project_id_of_conversation(&cid).as_deref(),
            Some(pa_id.as_str()),
            "会话必须归属于手机上选中的那个项目"
        );
        assert_eq!(
            crate::conv::project_work_dir(&pa_id).as_deref(),
            Some(alpha.to_string_lossy().as_ref()),
            "claude 的 cwd 就是这个值 —— 手机上说的活应该落在 alpha-repo 里"
        );

        // ⑤ 换到贝塔项目再开一条:cwd 必须跟着换,两条会话互不串目录
        let (c, conv2) = invoke(
            port,
            "conv_create_conversation",
            json!({"projectId": pb_id}),
        )
        .await;
        assert_eq!(c, 200, "body={conv2}");
        let cid2 = conv2["id"].as_str().unwrap().to_string();
        assert_eq!(
            crate::conv::project_id_of_conversation(&cid2).as_deref(),
            Some(pb_id.as_str())
        );
        assert_ne!(
            crate::conv::project_work_dir(&pa_id),
            crate::conv::project_work_dir(&pb_id),
            "两个项目应各在各的文件夹里"
        );

        // ⑥ 对照组 = 修复前的老行为:裸 `m-<ts>` 会话被挂到「第一个未归档项目」,
        //    跟用户选的那个项目毫无关系。这条固化下来,免得哪天回退了没人发现。
        let bare = format!("m-{}", 1_700_000_000_000u64);
        crate::conv::ensure_writable_or_create(&bare).unwrap();
        let default_pid = crate::conv::conv_list_projects()
            .first()
            .map(|p| p.id.clone())
            .unwrap();
        assert_eq!(
            crate::conv::project_id_of_conversation(&bare).as_deref(),
            Some(default_pid.as_str()),
            "裸会话落到第一个未归档项目 —— 这正是手机端非得显式建会话不可的原因"
        );

        let _ = stx.send(());
        std::env::remove_var("POLARIS_COLLAB_DB");
        let _ = std::fs::remove_file(&tmp);
        let _ = std::fs::remove_dir_all(&root);
    }
}

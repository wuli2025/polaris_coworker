/// 受控远程执行端点(B 方案)。刻意**不**并进 /api/invoke:
/// invoke 的闸是「命令在不在分发表」,exec 的闸是「总开关+模式+白名单」,
/// 两套语义混一个入口迟早互相漏。闸门顺序与理由见 exec.rs 头注释。
async fn exec_ep(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(req): Json<crate::exec::ExecRequest>,
) -> Response {
    let gate = OriginGate::closed();
    // 闸 2:fail-closed。没设访问口令且没强制登录 = 基础面免口令(合成 owner),
    // 那对 exec 就是**无鉴权 shell**。这里直接拒,且不提供开关绕过。
    // 注:这里刻意**不**走「内网免口令」(OriginGate::closed()),exec 必须拿到真凭据 ——
    // 内网免口令是为了让家用 NAS 用户能进对话/知识库,不是为了给局域网开一个无鉴权 shell。
    if effective_token(&state.auth_token).is_none() && !require_login_env() {
        crate::collab::db::audit(
            "anonymous",
            "exec.denied",
            &req.cmd,
            "开放模式(未设访问口令)",
        );
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error":"本机未设访问口令,远程执行已禁用。请给容器设 POLARIS_AUTH_TOKEN(或 POLARIS_REQUIRE_LOGIN=1)再用"})),
        )
            .into_response();
    }
    let Some(ctx) = app_ctx(&state, &headers, gate).await else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"未授权 (口令错误或会话失效)"})),
        )
            .into_response();
    };
    // 闸 3:**按设备**。会话上记着这条请求来自哪台设备,台账上那一排「远程执行:
    // 禁止/每次问/允许」在这里才真的生效。设备码进来的对端默认禁止 —— 拿到码不等于
    // 拿到一个 shell,那要机主在台账上单独点头。本机自己/全局口令不受此闸(device_id 为空)。
    let dev = crate::collab::http::bearer_of(&headers)
        .and_then(|t| crate::collab::auth::check_session_with_device(&t).ok())
        .map(|(_, d)| d)
        .unwrap_or_default();
    if let Err(e) = crate::collab::grants::exec_gate(&dev) {
        crate::collab::db::audit(&ctx.username, "exec.denied", &req.cmd, &e);
        return (StatusCode::FORBIDDEN, Json(json!({ "error": e }))).into_response();
    }
    // 角色闸只对**没有设备契约**的调用方生效:设备码进来的是 visitor,它的权限
    // 完全由上面那道契约闸说了算,再压一道 owner 角色等于把这条路彻底堵死。
    let by_grant = matches!(crate::collab::grants::get(&dev), Ok(Some(g)) if !g.revoked);
    if !by_grant && role_rank(&ctx.role) < 3 {
        crate::collab::db::audit(&ctx.username, "exec.denied", &req.cmd, "角色不足");
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error":"远程执行需要 owner 权限"})),
        )
            .into_response();
    }
    match crate::exec::run(&ctx.username, req).await {
        // 命令跑完即 200(哪怕退出码非 0)——那是命令的结果,不是接口的错误;
        // 调用方看 exit_code/ok 字段。只有闸门拒绝/启动失败才给非 2xx。
        Ok(r) => Json(r).into_response(),
        Err(e) => {
            // 闸门拒绝是 403(不是 500):调用方据此提示「让主机侧开开关/解锁 Shell」。
            let status = if e.contains("未开启远程执行")
                || e.contains("不在白名单")
                || e.contains("白名单模式")
                || e.contains("元字符")
                || e.contains("不能带路径")
            {
                StatusCode::FORBIDDEN
            } else if e.contains("缺少参数")
                || e.contains("工作目录不存在")
                || e.contains("启动失败")
            {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, Json(json!({ "error": e }))).into_response()
        }
    }
}

async fn invoke(
    State(state): State<ApiState>,
    peer: Option<ConnectInfo<SocketAddr>>,
    headers: HeaderMap,
    Json(req): Json<InvokeRequest>,
) -> Response {
    let gate = gate_of(&state, peer, &headers);
    let Some(ctx) = app_ctx(&state, &headers, gate).await else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"未授权 (口令错误或会话失效)"})),
        )
            .into_response();
    };
    if role_rank(&ctx.role) < required_role(&req.cmd) {
        crate::collab::db::audit(&ctx.username, "invoke.denied", &req.cmd, "角色不足");
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error": format!("权限不足:命令 {} 需要更高角色", req.cmd)})),
        )
            .into_response();
    }
    if ctx.user_id > 0 {
        let operation = crate::collab::interconnect::risk_inventory::HIGH_RISK_OPERATIONS.iter()
            .find(|op| op.entry_points.contains(&req.cmd.as_str())).map(|op| op.id);
        if let Some(operation) = operation {
            let token = bearer_of(&headers).map_or_else(String::new, |token| token);
            let payload = json!({"command":req.cmd,"args":req.args});
            let assertion = headers.get("x-polaris-device-assertion")
                .and_then(|value| value.to_str().ok()).map(String::from);
            match tokio::task::spawn_blocking(move || {
                crate::collab::interconnect::gate::session_proven(&token, operation, &payload, assertion.as_deref())
            }).await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => return invoke_err_resp(format!("(403) {error}")),
                Err(error) => return invoke_err_resp(error.to_string()),
            }
        }
    }
    let cmd = req.cmd;
    let args = req.args;
    let app = state.app();

    if cmd == "agent_backend_status" {
        return match crate::chat::agent_backend_status() {
            Ok(status) => Json(json!(status)).into_response(),
            Err(error) => invoke_err_resp(error),
        };
    }

    // chat_send 是 async（其余皆 sync）。单独处理。
    if cmd == "chat_send" {
        let inner = args.get("args").cloned().unwrap_or(Value::Null);
        let parsed: Result<crate::chat::ChatSendArgs, _> = serde_json::from_value(inner);
        return match parsed {
            Ok(a) => match crate::chat::chat_send(app, a).await {
                Ok(req_id) => Json(json!(req_id)).into_response(),
                Err(e) => invoke_err_resp(e),
            },
            Err(e) => invoke_err_resp(format!("chat_send 参数解析失败: {e}")),
        };
    }

    // ── 「隔空同屏」三条命令在这里就地处理,不进 dispatch ──────────────────────
    // 理由:beam_send 要往 **本次请求所属的广播总线**(state.tx)里投递,而 dispatch_* 只拿得到
    // AppHandle;桌面壳里 AppHandle 是 tauri 句柄,emit 只到本机 webview,手机永远收不到。
    // 在这一层处理还有个好处:双壳(desktop/server)共用同一段代码,不用在两个 dispatch 里各写一份。
    if cmd == "beam_send" {
        // 允许 {msg:{…}} 与直接把字段摊平两种写法(手机端少一层包装)。
        let raw = args
            .get("msg")
            .cloned()
            .unwrap_or(Value::Object(args.as_object().cloned().unwrap_or_default()));
        return match crate::beam::normalize(raw) {
            Ok(msg) => {
                let _ = state.tx.send(crate::host::Event::new(
                    crate::beam::TOPIC.to_string(),
                    msg.clone(),
                    None,
                ));
                crate::collab::db::audit(
                    &ctx.username,
                    "beam.send",
                    msg.get("act").and_then(|v| v.as_str()).unwrap_or(""),
                    msg.get("path").and_then(|v| v.as_str()).unwrap_or(""),
                );
                Json(json!({ "delivered": true, "msg": msg })).into_response()
            }
            Err(e) => invoke_err_resp(e),
        };
    }
    if cmd == "beam_pack" || cmd == "beam_export" {
        let path = match args.get("path").and_then(|v| v.as_str()) {
            Some(p) => p.to_string(),
            None => return invoke_err_resp("缺少字符串参数 `path`".into()),
        };
        let out_dir = args
            .get("outDir")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let want_export = cmd == "beam_export";
        // 打包要读文件 + base64 整份编码,是纯 CPU/IO 活,必须离开 async worker。
        let joined = tokio::task::spawn_blocking(move || {
            if want_export {
                crate::beam::export(&path, out_dir).map(|p| json!(p))
            } else {
                crate::beam::pack(&path)
                    .and_then(|d| serde_json::to_value(d).map_err(|e| e.to_string()))
            }
        })
        .await;
        return match joined {
            Ok(Ok(v)) => Json(v).into_response(),
            Ok(Err(e)) => invoke_err_resp(e),
            Err(e) => err_resp(format!("打包任务失败: {e}")),
        };
    }

    // 其余命令同步执行，丢到阻塞线程池（内含 ureq 网络/文件 IO，勿阻塞 async worker）。
    // 必须设超时：阻塞池只有 64 线程，慢命令无超时会一条条钉死线程。
    let timeout_secs: u64 = std::env::var("POLARIS_INVOKE_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(300);
    let cmd_for_err = cmd.clone();

    // 分发按 flavor 分裂:
    //  - server 壳:引擎命令是同步 fn → spawn_blocking 丢阻塞池,外套超时。
    //  - desktop 主机:引擎命令是 async 薄包装(内部自带 spawn_blocking)→ 直接 await,
    //    走精简 dispatch_desktop(覆盖手机数据面所需命令;全量命令用 Docker/NAS server 版)。
    // out 统一成 Result<Result<Value,String>, tokio::task::JoinError> 供下方一致处理。
    #[cfg(not(feature = "desktop"))]
    let out: Result<(Result<Value, String>, Vec<String>), tokio::task::JoinError> = {
        let fut = tokio::task::spawn_blocking(move || {
            let a = Args::new(args);
            let r = dispatch_sync(&cmd, &a, app);
            (r, a.unknown_keys())
        });
        if timeout_secs == 0 {
            fut.await
        } else {
            match tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), fut).await {
                Ok(joined) => joined,
                Err(_) => {
                    return (
                        StatusCode::GATEWAY_TIMEOUT,
                        Json(json!({
                            "error": format!(
                                "命令 {cmd_for_err} 执行超时({timeout_secs}s)，已停止等待（任务可能仍在后台运行）"
                            )
                        })),
                    )
                        .into_response();
                }
            }
        }
    };
    #[cfg(feature = "desktop")]
    let out: Result<(Result<Value, String>, Vec<String>), tokio::task::JoinError> = {
        let a = Args::new(args);
        let res = if timeout_secs == 0 {
            dispatch_desktop(&cmd, &a, app).await
        } else {
            match tokio::time::timeout(
                std::time::Duration::from_secs(timeout_secs),
                dispatch_desktop(&cmd, &a, app),
            )
            .await
            {
                Ok(r) => r,
                Err(_) => {
                    return (
                        StatusCode::GATEWAY_TIMEOUT,
                        Json(json!({
                            "error": format!(
                                "命令 {cmd_for_err} 执行超时({timeout_secs}s)，已停止等待（任务可能仍在后台运行）"
                            )
                        })),
                    )
                        .into_response();
                }
            }
        };
        Ok((res, a.unknown_keys()))
    };

    match out {
        Ok((Ok(v), unknown)) => {
            if unknown.is_empty() {
                return Json(v).into_response();
            }
            // 未知参数 = 客户端拼错名/契约漂移(top_k vs topK 一类),此前被静默容忍产生
            // 错误业务结果。默认曝光不破坏既有客户端;严格模式直接拒绝。
            if strict_args_enabled() {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": format!(
                        "未知参数: {}(命令 {} 未读取任何同名参数;各命令参数名以 tauri.ts 为准)",
                        unknown.join(", "), cmd_for_err
                    )})),
                )
                    .into_response();
            }
            let mut resp = Json(v).into_response();
            if let Ok(hv) = axum::http::HeaderValue::from_str(&unknown.join(",")) {
                resp.headers_mut().insert("x-polaris-unknown-args", hv);
            }
            resp
        }
        Ok((Err(e), _)) => invoke_err_resp(e),
        Err(e) => err_resp(format!("内部任务失败: {e}")),
    }
}

use super::*;

// ───────────────────────── WebSocket(emit 推流)─────────────────────────

/// WS 推流循环(server 壳的全量 /ws 与桌面 hosting 的协作 /ws 共用)。
/// audience 过滤:定向事件只投给受众本人;owner 全收。未标受众的机器级广播在尚无
/// team/project ACL 时只给 owner，避免聊天 delta、其它团队任务流泄露给任意登录者。
pub async fn ws_loop(socket: WebSocket, mut rx: broadcast::Receiver<Event>, ctx: AuthCtx) {
    let (mut sender, mut receiver) = socket.split();
    // 读侧:仅用于探测客户端关闭(前端浏览器模式不向后端 emit)。
    let mut closed = tokio::spawn(async move { while let Some(Ok(_)) = receiver.next().await {} });
    let is_owner = role_rank(&ctx.role) >= 3;

    loop {
        tokio::select! {
            recv = rx.recv() => match recv {
                Ok(ev) => {
                    // 按用户过滤(方案硬伤2):定向事件只投给受众本人;owner 全收(审计视角)。
                    match &ev.audience {
                        Some(aud) if !is_owner && aud != &ctx.username => continue,
                        None if !is_owner => continue,
                        _ => {}
                    }
                    // 帧在 emit 时已预序列化(Event::frame),N 条连接共享同一份 Arc<str>,
                    // 这里只剩 Arc<str> → String 一次拷贝(axum 0.7 Message::Text 要 String)。
                    if sender
                        .send(Message::Text((*ev.frame).to_string()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue, // 落后则跳过旧帧
                Err(broadcast::error::RecvError::Closed) => break,
            },
            _ = &mut closed => break,
        }
    }
    // 读探测任务必须显式收尸:tokio 的 JoinHandle 被 drop 只是「分离」不是「中止」,
    // 发送侧出错/频道关闭 break 后若不 abort,每断一条 WS 就残留一个永久挂在
    // receiver.next() 上的任务,7×24 下缓慢泄漏。(对已完成的任务 abort 是无害 no-op。)
    closed.abort();
}

/// 桌面 hosting 专用 /ws:走 collab 严格鉴权(会话 token / 全局口令 / 未启用时的
/// 单人兜底),不用 server 壳那套宽松的基础面鉴权(那是给单人 Docker chat 流的)。
pub(super) async fn collab_ws_handler(
    State(state): State<CollabState>,
    Query(params): Query<HashMap<String, String>>,
    ws: WebSocketUpgrade,
) -> Response {
    // 鉴权同步查库挪阻塞池(与 auth_ctx 同理;WS token 走 query 而非 header)。
    let auth_token = state.auth_token.clone();
    let token = params.get("token").cloned();
    let ctx = tokio::task::spawn_blocking(move || resolve_auth(&auth_token, token.as_deref()))
        .await
        .ok()
        .flatten();
    let Some(ctx) = ctx else {
        return (StatusCode::UNAUTHORIZED, "未授权").into_response();
    };
    let rx = state.app.subscribe();
    ws.on_upgrade(move |socket| ws_loop(socket, rx, ctx))
}

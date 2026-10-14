
// ───────────────────────── WebSocket（emit 推流）─────────────────────────

async fn ws_handler(
    State(state): State<ApiState>,
    peer: Option<ConnectInfo<SocketAddr>>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
    ws: WebSocketUpgrade,
) -> Response {
    // WS 鉴权走 query token（浏览器 WS 不便带自定义 header）。
    let gate = gate_of(&state, peer, &headers);
    let Some(ctx) = resolve_app_auth(&state, params.get("token").cloned(), gate).await else {
        return (StatusCode::UNAUTHORIZED, "未授权").into_response();
    };
    if role_rank(&ctx.role) < 3 {
        return (StatusCode::FORBIDDEN, "基础事件流需要 owner 权限").into_response();
    }
    let rx = state.tx.subscribe();
    ws.on_upgrade(move |socket| ws_loop(socket, rx, ctx))
}

// ───────────────────────── 文件上传（替代原生文件对话框）─────────────────────────

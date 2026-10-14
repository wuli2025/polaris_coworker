/// 基础面鉴权核心:按访问口令 + 传入 token 解析身份。server.rs 的壳专属端点
/// (/api/status 等)也复用它,故 pub(crate)、且不吃 State(只吃 auth_token 引用)。
pub(crate) fn resolve_app_auth_token(
    auth_token: &Option<String>,
    token: Option<&str>,
    gate: OriginGate,
) -> Option<AuthCtx> {
    // 全局口令命中 = owner(单人 Docker 管理员)。口令只来自 POLARIS_AUTH_TOKEN,
    // 网页里再也设不出第二个来源(见 effective_token 上面那段)。
    let effective = effective_token(auth_token);
    if let Some(expected) = effective.as_ref() {
        if token == Some(expected.as_str()) {
            return Some(AuthCtx {
                user_id: 0,
                username: "admin".into(),
                role: "owner".into(),
                device_id: String::new(),
            });
        }
    }
    // 带了有效会话 token → 用真实身份(多用户下据此过角色闸)。
    if let Some(t) = token {
        if let Ok((u, device_id)) = crate::collab::auth::check_session_with_device(t) {
            return Some(AuthCtx {
                user_id: u.id,
                username: u.username,
                role: u.role,
                device_id,
            });
        }
    }
    // 免口令模式:gate.enabled 由调用方算出,成立的条件是 —— 本壳是 Docker server 壳、
    // 没设 POLARIS_AUTH_TOKEN、也没开 POLARIS_REQUIRE_LOGIN。此时任何能连上的来源
    // 直接是 owner,不弹任何框(家用 NAS 的日常)。
    // 唯一的例外是管理员显式打开 POLARIS_LAN_ONLY=1:那时 origin_ok 才真去判内外网,
    // 公网来源在这里 **拒**,绝不能掉到下面的「开放模式」兜底里去 —— 否则那个开关白设。
    if gate.enabled {
        return gate.origin_ok.then(|| AuthCtx {
            user_id: 0,
            username: "local".into(),
            role: "owner".into(),
            device_id: String::new(),
        });
    }
    // 是否强制登录:设了全局口令,或显式打开 POLARIS_REQUIRE_LOGIN。
    let require_login = effective.is_some() || require_login_env();
    if require_login {
        return None; // 上面没拿到有效凭据 → 拒绝
    }
    // 开放模式:合成 owner(历史行为,单人场景全放行)。
    Some(AuthCtx {
        user_id: 0,
        username: "local".into(),
        role: "owner".into(),
        device_id: String::new(),
    })
}

/// 从请求头(Bearer)解析基础面身份。server.rs 壳专属端点复用,故 pub(crate)+同门控。
#[cfg(feature = "server")]
pub(crate) fn app_ctx_headers(
    auth_token: &Option<String>,
    headers: &HeaderMap,
    gate: OriginGate,
) -> Option<AuthCtx> {
    resolve_app_auth_token(auth_token, bearer_of(headers).as_deref(), gate)
}

/// server.rs 壳专属端点用的来源判定(它拿不到 ApiState,只有 AppState)。
#[cfg(feature = "server")]
pub(crate) fn server_origin_gate(
    open_no_auth: bool,
    peer: Option<SocketAddr>,
    headers: &HeaderMap,
) -> OriginGate {
    origin_gate(open_no_auth, peer, headers)
}

/// 协作面 `bootstrap`(建第一个账号)专用的来源闸。
///
/// 与 [`server_origin_gate`] 只差一点：**不看 `POLARIS_REQUIRE_LOGIN`**。
/// 那个开关的意思是「所有请求都要登录账号」，可库里零账号时根本没有账号可登 ——
/// 若连建号也按它拦掉，`REQUIRE_LOGIN=1` + 没设口令的新机器就永久锁死了
/// (旧版靠自动生成的随机口令兜住，免口令改动把那条路撤了)。
/// `POLARIS_LAN_ONLY` 照旧生效：它管的是「谁够得着」，与要不要登录是两回事。
#[cfg(feature = "server")]
pub(crate) fn server_bootstrap_gate(
    open_no_auth: bool,
    peer: Option<SocketAddr>,
    headers: &HeaderMap,
) -> OriginGate {
    origin_gate_with(open_no_auth, lan_only_env(), peer, headers)
}

/// resolve_app_auth_token 的异步壳:鉴权含同步 SQLite 查询(check_session),直接跑在
/// axum async worker 上会钉住 reactor,挪进阻塞线程池。会话短缓存(collab/auth.rs)命中
/// 时闭包内不落库,这层 spawn_blocking 兜的是未命中/首次。
async fn resolve_app_auth(
    state: &ApiState,
    token: Option<String>,
    gate: OriginGate,
) -> Option<AuthCtx> {
    let auth_token = state.auth_token.clone();
    tokio::task::spawn_blocking(move || resolve_app_auth_token(&auth_token, token.as_deref(), gate))
        .await
        .ok()
        .flatten()
}

/// 端点侧的免口令判定入口:把 ConnectInfo(可能没挂)+ 头 交给 origin_gate。
/// 「本壳允许」之外再加一条硬条件:**这台机器没设访问口令**。设了就全员校验。
fn gate_of(
    state: &ApiState,
    peer: Option<ConnectInfo<SocketAddr>>,
    headers: &HeaderMap,
) -> OriginGate {
    let enabled = state.open_no_auth && effective_token(&state.auth_token).is_none();
    origin_gate(enabled, peer.map(|c| c.0), headers)
}

async fn app_ctx(state: &ApiState, headers: &HeaderMap, gate: OriginGate) -> Option<AuthCtx> {
    resolve_app_auth(state, bearer_of(headers), gate).await
}

// ── /api/auth/state 与 /api/auth/setup ────────────────────────────────────
//
// 两个端点都**不鉴权**(没口令可鉴)。/state 是只读探针,前端据它决定要不要弹口令框;
// /setup 曾经是「首次访问向导」的落盘入口,现已停用(见上面「为什么默认不要访问口令」),
// 保留路由只为给老前端一个说得清的错,而不是 404。

#[derive(serde::Serialize)]
struct AuthState {
    /// 这台机器是否设了访问口令(即有没有 POLARIS_AUTH_TOKEN)。
    initialized: bool,
    /// 口令只可能由 POLARIS_AUTH_TOKEN 管着,网页里改不了。恒等于 initialized。
    env_managed: bool,
    /// 网页端能不能设口令。**恒 false**:首次访问向导已停用。
    can_setup: bool,
    /// 当前访客是不是靠「免口令」进来的。
    open_to_me: bool,
}

async fn auth_state(
    State(state): State<ApiState>,
    peer: Option<ConnectInfo<SocketAddr>>,
    headers: HeaderMap,
) -> Response {
    let env_managed = state.auth_token.is_some();
    let initialized = effective_token(&state.auth_token).is_some();
    let gate = gate_of(&state, peer, &headers);
    Json(AuthState {
        initialized,
        env_managed,
        can_setup: false,
        open_to_me: gate.allows(),
    })
    .into_response()
}

#[derive(serde::Deserialize)]
struct SetupReq {
    #[allow(dead_code)] // 端点已停用,请求体只为兼容老前端而保留
    token: String,
}

async fn auth_setup(Json(_req): Json<SetupReq>) -> Response {
    // 410 而不是 403:这不是权限不够,是这个功能没了。老前端(缓存的旧 index.html)
    // 撞上它会直接放弃向导,不会把用户卡在一个永远失败的弹框里。
    (
        StatusCode::GONE,
        Json(json!({
            "error": "网页端设置访问口令已停用:本机默认免口令。要上锁请给容器设 POLARIS_AUTH_TOKEN"
        })),
    )
        .into_response()
}

/// 基础 `/api/invoke` 目前操作机器级项目/知识库/供应商配置,没有逐用户资源 ACL;
/// chat 还可调用 Bash/PowerShell。ACL 完成前统一 fail-closed 为 owner;团队成员只用 `/api/collab/*`。
fn required_role(_cmd: &str) -> u8 {
    3
}

// ───────────────────────── /api/invoke 分发 ─────────────────────────

// 信封与参数记账下沉契约层(polaris-protocol, 分仓规划 v2 第 1 仓种子):
// Args 记录分发代码实际读过哪些顶层参数, 没被读的 = 拼错名/契约漂移 —— 默认经
// `x-polaris-unknown-args` 响应头曝光, POLARIS_STRICT_ARGS=1 时直接 400。
use polaris_protocol::{strict_args_enabled, Args, InvokeRequest};

/// 把命令错误串按「客户端错误 vs 服务端错误」映射到合适的 HTTP 状态码,而非一律 500。
fn invoke_err_resp(e: String) -> Response {
    let status = if e.starts_with("未知命令") {
        StatusCode::NOT_FOUND
    } else if (e.contains("参数")
        && (e.contains("缺少") || e.contains("解析失败") || e.contains("无效")))
        // 非法枚举值(如 fable_search「mode 只接受 hybrid | grep | vector」)是客户端错误,
        // 此前落进兜底 500(spot-check 揪出错误分类)。
        || e.contains("只接受")
    {
        StatusCode::BAD_REQUEST
    } else if e.contains("(403)") {
        StatusCode::FORBIDDEN
    } else if e.contains("(404)") {
        StatusCode::NOT_FOUND
    } else if e.contains("(429)") {
        StatusCode::TOO_MANY_REQUESTS
    } else if e.contains("insufficient") || e.contains("余额") {
        // 上游供应商余额/额度失败(如云嵌入 403 account balance):外部依赖失败,
        // 非本服务端 bug。映射 502 让客户端提示「换供应商/充值」而非报「服务器崩了」。
        StatusCode::BAD_GATEWAY
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    };
    (status, Json(json!({ "error": e }))).into_response()
}

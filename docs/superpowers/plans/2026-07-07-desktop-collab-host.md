# 桌面一键当主机 + 配对码带地址 + 设备页主机徽标 — 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 桌面版点一颗按钮就把本机变成协作主机(内嵌 axum 只挂协作端点),邀请配对码里携带主机地址让成员零填写入伙,设备管理页标出哪台是主机。

**Architecture:** 把 server.rs 里的全部 collab HTTP handler(全是薄包装,真逻辑在 `crate::collab::*`)抽成双壳共用的 `collab/http.rs`(新 feature `collab-host` 门控 axum 依赖;`server` feature 改为蕴含它,`desktop` 也加入它)。桌面新增 `collab/hosting.rs`:`#[tauri::command]` 起/停内嵌 axum(默认端口 8484,只挂 `/api/collab/*`、`/git/*`、`/ws`、`/api/health`,不碰 kb/chat 等已由桌面 setup 初始化的模块),事件走独立 broadcast 频道,另起桥接任务把事件转发给本机 Tauri UI。配对码升级为 `PLRS1-<base64url(json)>` 分享码,内含裸码+主机可达地址列表;兑换端解析→逐个探活→自动 setBase。主机身份落 collab.db 新 meta 表(`host_node_id`),设备列表响应附 `is_host`。

**Tech Stack:** Rust(axum 0.7 / tokio / rusqlite / base64 0.22 已有依赖,零新增 crate)、Vue3 + Pinia、Tauri 2。

**前置事实(侦察结论,写码前不用再验证):**
- collab 逻辑模块(auth/db/identity/teams/projects/tasks/lead/lead_ai/mergectl/gitea/account_store/workset)无 feature 门控,双壳可用。`collab/commands.rs` 仅 desktop、`collab/tunnel.rs` 仅 collab-net。
- server.rs 的 collab handler 模式统一:`auth_ctx()` → `role_rank`/`ensure_member` 闸 → `spawn_blocking` 调 `crate::collab::xxx` → `unwrap_api` 包装。依赖 AppState 的只有 `auth_token`(经 auth_ctx)与 `app`(emit 广播)。
- `host.rs`(75 行)= `broadcast::Sender<Event>` 的 shim,`Event{topic,payload,audience}`;现被 `#[cfg(feature="server")]` 门控,但其依赖(tokio/serde)是无条件依赖,可直接去门控。
- 票据:8 位大写码(去 0O1I),tickets 表,24h TTL,redeem 原子占用后 create_user+add_device+login。`create_ticket(role,note)` 在 identity.rs:98。
- collab.db 路径:`POLARIS_COLLAB_DB` env 覆写(测试用),否则 `~/Polaris/data/collab.db`;`migrate()` 在 db.rs:51,`CREATE TABLE IF NOT EXISTS` 风格,幂等。
- 前端:`api.ts` 的 `getBase/setBase`(localStorage `polaris.collab.base.v1`)、`deviceId()`;`stores/collab.ts` 的 `needsHost = isTauri && !base`;`CollabView.vue` doAuth 已有 needsHost 拦截;`CollabAdmin.vue` 票据展示 `.tk-code` 只显示裸码,设备表四列无主机字段;`tauri.ts` 导出 `invoke<T>(cmd, args)`(桌面直调 Tauri 命令)。
- **Windows 编译坑:** cargo build 前必须先杀掉在跑的 `polaris-app.exe` / `polaris-server.exe`,否则链接期报 exe 被锁。
- **提交纪律:** 仓库有大量本任务之外的未提交改动,**只 `git add` 本计划点名的文件,严禁 `git add -A`**。

---

### Task 1: feature 布线 + host.rs 去门控 + http.rs 骨架

**Files:**
- Modify: `src-tauri/Cargo.toml`(features 段)
- Modify: `src-tauri/src/lib.rs:47-51`(host/server 模块门控)
- Modify: `src-tauri/src/collab/mod.rs`
- Create: `src-tauri/src/collab/http.rs`

- [ ] **Step 1: Cargo.toml features 改三行**

`src-tauri/Cargo.toml` `[features]` 段(现 :147-178),将:

```toml
desktop = [
    "dep:tauri",
    "dep:tauri-plugin-dialog",
    "dep:tauri-plugin-fs",
    "dep:tauri-plugin-shell",
    "dep:tauri-plugin-updater",
    "dep:tauri-plugin-process",
    "dep:polaris-sandbox",
    "dep:headless_chrome",   # forge_capture fallback tier2(可选)
    "custom-protocol",
]
```
改为(尾部加一项):
```toml
desktop = [
    "dep:tauri",
    "dep:tauri-plugin-dialog",
    "dep:tauri-plugin-fs",
    "dep:tauri-plugin-shell",
    "dep:tauri-plugin-updater",
    "dep:tauri-plugin-process",
    "dep:polaris-sandbox",
    "dep:headless_chrome",   # forge_capture fallback tier2(可选)
    "custom-protocol",
    "collab-host",           # 桌面一键当主机:内嵌协作 axum 路由
]
```
将 `server = ["dep:axum", "dep:tower", "dep:tower-http", "dep:futures-util"]` 改为:
```toml
# Docker/Web 构建:axum HTTP/WS 外壳。collab-host 提供共用的协作路由与 axum 依赖。
server = ["collab-host"]
# 协作路由双壳共用(collab/http.rs)+ 桌面内嵌主机(collab/hosting.rs)所需的 HTTP 依赖。
collab-host = ["dep:axum", "dep:tower", "dep:tower-http", "dep:futures-util"]
```

- [ ] **Step 2: lib.rs 模块门控调整**

`src-tauri/src/lib.rs:47-51`,将:
```rust
// ── Docker(server) 外壳：shim AppHandle + axum HTTP/WS 服务 ──
#[cfg(feature = "server")]
pub mod host;
#[cfg(feature = "server")]
pub mod server;
```
改为:
```rust
// ── host shim(broadcast 事件壳):server 壳与桌面内嵌协作主机(collab-host)共用 ──
pub mod host;
// ── Docker(server) 外壳：axum HTTP/WS 服务 ──
#[cfg(feature = "server")]
pub mod server;
```

- [ ] **Step 3: collab/mod.rs 挂新模块**

`src-tauri/src/collab/mod.rs`,在 `#[cfg(feature = "desktop")] pub mod commands;` 附近加:
```rust
/// 协作 HTTP 路由(axum,双壳共用):server 壳 merge 它;桌面 hosting 内嵌它。
#[cfg(feature = "collab-host")]
pub mod http;
```

- [ ] **Step 4: 建 http.rs 骨架(状态类型 + 地址探测 + 空路由)**

Create `src-tauri/src/collab/http.rs`:
```rust
//! 多人协作 HTTP 路由 —— 双壳共用(Docker server 壳 merge;桌面 hosting 内嵌)。
//! Task 2 会把 server.rs 里的全部 collab handler 迁进来;本文件先立地基:
//! CollabState / 分享码地址探测 / collab_router 骨架。

use crate::host::AppHandle;
use axum::Router;
use std::sync::Arc;

/// 协作面状态:与 server::AppState 解耦,桌面内嵌时独立构造。
#[derive(Clone)]
pub struct CollabState {
    /// 事件广播壳(server 壳与其 AppState.app 同源;桌面 hosting 独立频道)
    pub app: AppHandle,
    /// 全局访问口令(POLARIS_AUTH_TOKEN);None = 未设
    pub auth_token: Arc<Option<String>>,
    /// 票据分享码携带的「本机可达地址」(http://ip:port),外壳启动时注入
    pub advertise: Arc<parking_lot::RwLock<Vec<String>>>,
}

/// 探测本机可对外通告的地址。优先 POLARIS_ADVERTISE_URL(逗号分隔,给反代/固定域名用);
/// 否则 UDP connect 技巧取「默认路由出口 IP」与「Tailscale 口 IP」(connect 不发包,零依赖)。
pub fn detect_advertise_urls(port: u16) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(v) = std::env::var("POLARIS_ADVERTISE_URL") {
        for s in v.split(',') {
            let s = s.trim();
            if !s.is_empty() {
                out.push(s.trim_end_matches('/').to_string());
            }
        }
        if !out.is_empty() {
            return out;
        }
    }
    let probe = |target: &str| -> Option<std::net::IpAddr> {
        let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
        s.connect(target).ok()?;
        s.local_addr().ok().map(|a| a.ip())
    };
    let mut ips: Vec<std::net::IpAddr> = Vec::new();
    if let Some(ip) = probe("8.8.8.8:80") {
        ips.push(ip); // 默认路由(局域网/公网口)
    }
    if let Some(ip) = probe("100.100.100.100:80") {
        if !ips.contains(&ip) {
            ips.push(ip); // Tailscale 口(100.64/10 路由存在才命中)
        }
    }
    for ip in ips {
        if !ip.is_loopback() {
            out.push(format!("http://{ip}:{port}"));
        }
    }
    out
}

/// 协作路由。`with_ws=true` 时附带 /ws(桌面 hosting 用;server 壳自己有全量 /ws,传 false 防 merge 撞路由)。
/// Task 2 迁入全部 /api/collab/* 与 /git/* 路由;本任务先返回空 Router 保证双壳编译。
pub fn collab_router(state: CollabState, with_ws: bool) -> Router {
    let _ = with_ws; // Task 2 使用
    Router::new().with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advertise_env_override_wins() {
        std::env::set_var("POLARIS_ADVERTISE_URL", "http://nas.example:9000/, https://p.example");
        let urls = detect_advertise_urls(8484);
        std::env::remove_var("POLARIS_ADVERTISE_URL");
        assert_eq!(urls, vec!["http://nas.example:9000".to_string(), "https://p.example".to_string()]);
    }

    #[test]
    fn advertise_autodetect_no_loopback() {
        std::env::remove_var("POLARIS_ADVERTISE_URL");
        for u in detect_advertise_urls(8484) {
            assert!(u.starts_with("http://"));
            assert!(!u.contains("127.0.0.1"));
            assert!(u.ends_with(":8484"));
        }
    }
}
```

- [ ] **Step 5: 双壳编译 + 跑新单测**

先杀进程:`Get-Process polaris-app,polaris-server -ErrorAction SilentlyContinue | Stop-Process -Force`
```powershell
cargo check --manifest-path src-tauri\Cargo.toml
cargo check --manifest-path src-tauri\Cargo.toml --no-default-features --features server
cargo test --manifest-path src-tauri\Cargo.toml --lib collab::http
```
Expected: 两个 check 无 error;test 2 passed。

- [ ] **Step 6: Commit**
```powershell
git add src-tauri/Cargo.toml src-tauri/src/lib.rs src-tauri/src/collab/mod.rs src-tauri/src/collab/http.rs
git commit -m "feat(collab): collab-host feature 布线 + 共用 HTTP 路由骨架(CollabState/地址探测)"
```

---

### Task 2: 把 collab handler 从 server.rs 整体迁入 http.rs

**Files:**
- Modify: `src-tauri/src/server.rs`(删 collab 段,改 merge + import)
- Modify: `src-tauri/src/collab/http.rs`(收编 handler)

这是纯机械搬迁:**逻辑零改动,只换状态类型**。server.rs 中按函数名定位(行号会漂移,勿按行数硬切)。

- [ ] **Step 1: 迁鉴权与工具函数**

从 server.rs 剪切以下项到 http.rs,全部标 `pub`(server.rs 其余 handler 还要用):
`AuthCtx` 结构体、`bearer_of`、`resolve_auth`、`auth_ctx`、`role_rank`、`forbid`、`ok`、`err_resp`、`unwrap_api`、`s_of`、`i_of`(及同族取参小工具,以编译器缺啥搬啥为准)、`ensure_member`、`is_team_admin`、`emit_task`、宏 `task_op!`。

签名改造(仅状态类型):
- `resolve_auth(state: &AppState, ...)` → `pub fn resolve_auth(auth_token: &Option<String>, token: Option<&str>) -> Option<AuthCtx>`(体内 `state.auth_token` 改参数直用;`crate::collab::auth::check_session` 调用不变)。
- `auth_ctx(state: &AppState, headers)` → `pub fn auth_ctx(state: &CollabState, headers: &HeaderMap) -> Option<AuthCtx>`,体内 `resolve_auth(&state.auth_token, bearer_of(headers).as_deref())`。
- `emit_task` 里 `state.app.emit(...)` 保持原样(CollabState.app 同为 host::AppHandle)。

server.rs 顶部加:
```rust
use crate::collab::http::{
    app_wide_reexports_as_needed, // 按编译器报错补全:ok, err_resp, unwrap_api, s_of, bearer_of, AuthCtx, role_rank ...
};
```
(实际写成具名列表;`resolve_app_auth`/`app_ctx`/`check_auth`/`required_role` 是基础应用面鉴权,**留在 server.rs**,其中对 `bearer_of`/`AuthCtx` 的引用改从 http.rs import。)

- [ ] **Step 2: 迁全部 collab handler(56 个,名单如下)+ /ws 循环**

剪切 server.rs 中这些 `async fn` 到 http.rs(除状态提取 `State<AppState>` → `State<CollabState>` 外零改动;`#[cfg(feature = "collab-net")]` 的 tunnel 三件与 stub 原样保留):

`collab_bootstrap, collab_login, collab_logout, collab_me, collab_redeem, collab_signup, collab_user_search, team_list, team_create, team_members_api, team_member_add, team_member_remove, collab_ticket, collab_users, collab_user_disable, collab_devices, collab_device_revoke, mirror_export_api, mirror_store_api, mirror_pull_api, mirror_restore_api, project_list, project_create, project_members, project_member_add, project_set_lead, task_list, task_create, task_claim, task_submit, task_review, task_rounds, task_archive, task_cancel, lead_grants_get, lead_grants_set, lead_morning, lead_assign_api, lead_nudge_api, lead_model_get, lead_model_set, lead_ai_decompose, lead_ai_review, lead_ai_fuse, merge_trial_api, merge_resolve_api, merge_squash_api, merge_revert_api, gitea_status_api, gitea_start_api, gitea_stop_api, git_proxy, tunnel_status_api, tunnel_start_api, tunnel_relays_api`

另把 `ws_loop`(server.rs:2130 附近)抽为 http.rs 的:
```rust
pub async fn ws_loop(socket: axum::extract::ws::WebSocket, ctx: AuthCtx, rx: tokio::sync::broadcast::Receiver<crate::host::Event>)
```
(体内 audience 过滤逻辑原样)。server.rs 的 `ws_handler` 保持用 `resolve_app_auth`(基础面宽松鉴权,**不许改**,否则 Docker 单人版 bootstrap 后 chat 流断),`on_upgrade` 里改调 `crate::collab::http::ws_loop(socket, ctx, state.tx.subscribe())`。

http.rs 里新增 hosting 专用的严格版 ws handler(走 collab 会话鉴权):
```rust
async fn collab_ws_handler(
    State(state): State<CollabState>,
    Query(q): Query<std::collections::HashMap<String, String>>,
    ws: axum::extract::WebSocketUpgrade,
) -> Response {
    let Some(ctx) = resolve_auth(&state.auth_token, q.get("token").map(|s| s.as_str())) else {
        return forbid();
    };
    let rx = state.app.subscribe();
    ws.on_upgrade(move |socket| ws_loop(socket, ctx, rx))
}
```

- [ ] **Step 3: collab_router 填真路由**

http.rs 的 `collab_router` 改为(路由表 = server.rs :115-170 的 collab 段原样搬):
```rust
pub fn collab_router(state: CollabState, with_ws: bool) -> Router {
    let mut r = Router::new()
        .route("/api/collab/bootstrap", post(collab_bootstrap))
        .route("/api/collab/login", post(collab_login))
        .route("/api/collab/logout", post(collab_logout))
        .route("/api/collab/me", get(collab_me))
        .route("/api/collab/redeem", post(collab_redeem))
        .route("/api/collab/signup", post(collab_signup))
        .route("/api/collab/users/search", get(collab_user_search))
        .route("/api/collab/teams", get(team_list).post(team_create))
        .route("/api/collab/team/members", get(team_members_api).post(team_member_add))
        .route("/api/collab/team/member_remove", post(team_member_remove))
        .route("/api/collab/admin/ticket", post(collab_ticket))
        .route("/api/collab/admin/users", get(collab_users))
        .route("/api/collab/admin/user_disable", post(collab_user_disable))
        .route("/api/collab/admin/devices", get(collab_devices))
        .route("/api/collab/admin/device_revoke", post(collab_device_revoke))
        .route("/api/collab/mirror/export", post(mirror_export_api))
        .route("/api/collab/mirror/store", post(mirror_store_api))
        .route("/api/collab/mirror/pull", get(mirror_pull_api))
        .route("/api/collab/mirror/restore", post(mirror_restore_api))
        .route("/api/collab/projects", get(project_list).post(project_create))
        .route("/api/collab/project/members", get(project_members).post(project_member_add))
        .route("/api/collab/project/lead", post(project_set_lead))
        .route("/api/collab/tasks", get(task_list).post(task_create))
        .route("/api/collab/task/claim", post(task_claim))
        .route("/api/collab/task/submit", post(task_submit))
        .route("/api/collab/task/review", post(task_review))
        .route("/api/collab/task/rounds", get(task_rounds))
        .route("/api/collab/task/archive", post(task_archive))
        .route("/api/collab/task/cancel", post(task_cancel))
        .route("/api/collab/lead/grants", get(lead_grants_get).post(lead_grants_set))
        .route("/api/collab/lead/morning", get(lead_morning))
        .route("/api/collab/lead/assign", post(lead_assign_api))
        .route("/api/collab/lead/nudge", post(lead_nudge_api))
        .route("/api/collab/lead/model", get(lead_model_get).post(lead_model_set))
        .route("/api/collab/lead/ai/decompose", post(lead_ai_decompose))
        .route("/api/collab/lead/ai/review", post(lead_ai_review))
        .route("/api/collab/lead/ai/fuse", post(lead_ai_fuse))
        .route("/api/collab/merge/trial", post(merge_trial_api))
        .route("/api/collab/merge/resolve", post(merge_resolve_api))
        .route("/api/collab/merge/squash", post(merge_squash_api))
        .route("/api/collab/merge/revert", post(merge_revert_api))
        .route("/api/collab/gitea/status", get(gitea_status_api))
        .route("/api/collab/gitea/start", post(gitea_start_api))
        .route("/api/collab/gitea/stop", post(gitea_stop_api))
        .route("/git/*rest", axum::routing::any(git_proxy))
        .route("/api/collab/tunnel/status", get(tunnel_status_api))
        .route("/api/collab/tunnel/start", post(tunnel_start_api))
        .route("/api/collab/tunnel/relays", post(tunnel_relays_api));
    if with_ws {
        r = r.route("/ws", get(collab_ws_handler));
    }
    r.with_state(state)
}
```
(若个别路由名与 server.rs 现状有出入,以 server.rs 现状为准 —— 整段剪过来。)

- [ ] **Step 4: server.rs 改 merge**

server.rs `serve()` 里删掉已迁走的全部 collab `.route(...)` 行,在 Router 构造后加:
```rust
let collab_state = crate::collab::http::CollabState {
    app: state.app.clone(),
    auth_token: state.auth_token.clone(),
    advertise: Arc::new(parking_lot::RwLock::new(
        crate::collab::http::detect_advertise_urls(port),
    )),
};
let app_router = app_router.merge(crate::collab::http::collab_router(collab_state, false));
```
注意 `port` 在现文件 :179 才解析 —— 把端口解析上移到 Router 构造之前。CORS/DefaultBodyLimit 等 `.layer` 在 merge 之后调用即可作用于全部路由(保持现状顺序:先 route/merge 后 layer)。

- [ ] **Step 5: 双壳编译 + 全量单测**
```powershell
cargo check --manifest-path src-tauri\Cargo.toml
cargo check --manifest-path src-tauri\Cargo.toml --no-default-features --features server
cargo test --manifest-path src-tauri\Cargo.toml --no-default-features --features server
```
Expected: 无 error;既有 collab 单测(19 个)全过。

- [ ] **Step 6: server 壳冒烟(证明搬迁没伤行为)**
```powershell
Get-Process polaris-server -ErrorAction SilentlyContinue | Stop-Process -Force
cargo build --manifest-path src-tauri\Cargo.toml -p polaris-cli --bin polaris-server
$env:POLARIS_COLLAB_DB = "$env:TEMP\collab-smoke.db"; Remove-Item $env:POLARIS_COLLAB_DB -Force -ErrorAction SilentlyContinue
Start-Process -FilePath src-tauri\target\debug\polaris-server.exe
Start-Sleep 4
curl.exe -s -X POST http://127.0.0.1:8080/api/collab/bootstrap -H "Content-Type: application/json" -d '{"username":"smoke","password":"pass1234","displayName":"S","deviceId":"d1"}'
```
Expected: 返回 `{"token":"...","user":{...}}`。完事 `Stop-Process`、清掉 `POLARIS_COLLAB_DB`。

- [ ] **Step 7: Commit**
```powershell
git add src-tauri/src/server.rs src-tauri/src/collab/http.rs
git commit -m "refactor(collab): HTTP 路由从 server 壳抽为双壳共用 collab/http.rs(逻辑零改动)"
```

---

### Task 3: 分享码(配对码带地址)

**Files:**
- Modify: `src-tauri/src/collab/identity.rs`(encode/decode + 测试)
- Modify: `src-tauri/src/collab/http.rs`(collab_ticket 返回 share)

- [ ] **Step 1: identity.rs 加编解码 + 失败样例测试**

identity.rs 追加(base64 0.22 已是依赖):
```rust
use base64::Engine as _;

/// 分享码:PLRS1-<base64url(json{c:裸码, a:[地址]})>。裸码仍单独入库,分享码只是传输壳。
pub fn encode_share_code(code: &str, addrs: &[String]) -> String {
    let payload = serde_json::json!({ "c": code, "a": addrs });
    let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload.to_string());
    format!("PLRS1-{b64}")
}

pub fn decode_share_code(s: &str) -> Option<(String, Vec<String>)> {
    let b64 = s.trim().strip_prefix("PLRS1-")?;
    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(b64).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&raw).ok()?;
    let code = v.get("c")?.as_str()?.to_string();
    let addrs = v
        .get("a")?
        .as_array()?
        .iter()
        .filter_map(|x| x.as_str().map(String::from))
        .collect();
    Some((code, addrs))
}
```
测试(identity.rs 已有 tests 模块就并入,没有就新建):
```rust
#[test]
fn share_code_roundtrip() {
    let addrs = vec!["http://192.168.1.5:8484".to_string(), "http://100.1.2.3:8484".to_string()];
    let s = encode_share_code("ABCD2345", &addrs);
    assert!(s.starts_with("PLRS1-"));
    let (code, back) = decode_share_code(&s).unwrap();
    assert_eq!(code, "ABCD2345");
    assert_eq!(back, addrs);
}

#[test]
fn share_code_rejects_garbage() {
    assert!(decode_share_code("ABCD2345").is_none());        // 裸码不是分享码
    assert!(decode_share_code("PLRS1-!!!not-b64").is_none()); // 坏 base64
}
```

- [ ] **Step 2: 跑测试**
`cargo test --manifest-path src-tauri\Cargo.toml --lib collab::identity`
Expected: share_code 两测过。

- [ ] **Step 3: collab_ticket 返回 share 字段**

http.rs 的 `collab_ticket`,把 `create_ticket(...).and_then(|t| ok(t))` 一段改为先克隆通告地址再组装(spawn_blocking 闭包外先 `let advertise = state.advertise.read().clone();`):
```rust
let advertise = state.advertise.read().clone();
let out = tokio::task::spawn_blocking(move || {
    let role = { let r = s_of(&v, "role"); if r.is_empty() { "collaborator".into() } else { r } };
    let t = crate::collab::identity::create_ticket(&role, &s_of(&v, "note"))?;
    let share = crate::collab::identity::encode_share_code(&t.code, &advertise);
    ok(serde_json::json!({ "code": t.code, "role": t.role, "expires_at": t.expires_at, "share": share }))
})
.await;
unwrap_api(out)
```

- [ ] **Step 4: server 壳冒烟验证 share**

同 Task 2 Step 6 起 server(临时库),bootstrap 拿 token 后:
```powershell
curl.exe -s -X POST http://127.0.0.1:8080/api/collab/admin/ticket -H "Authorization: Bearer <token>" -H "Content-Type: application/json" -d '{"role":"collaborator","note":"t"}'
```
Expected: 响应含 `"share":"PLRS1-..."`,base64 解开含本机局域网 IP(或 POLARIS_ADVERTISE_URL 值)。

- [ ] **Step 5: Commit**
```powershell
git add src-tauri/src/collab/identity.rs src-tauri/src/collab/http.rs
git commit -m "feat(collab): 邀请票据升级分享码 PLRS1-*,内嵌主机可达地址,成员零填写入伙"
```

---

### Task 4: meta 表 + bootstrap 登记主机设备 + 设备列表 is_host

**Files:**
- Modify: `src-tauri/src/collab/db.rs`(meta 表 + get/set)
- Modify: `src-tauri/src/collab/http.rs`(collab_bootstrap / collab_devices)

- [ ] **Step 1: db.rs 建 meta 表 + 助手 + 测试**

`migrate()` 里追加一条(与现有建表语句并列):
```rust
"CREATE TABLE IF NOT EXISTS meta(k TEXT PRIMARY KEY, v TEXT NOT NULL)",
```
db.rs 追加公开函数:
```rust
/// meta 键值(主机标识等全库级小状态)。
pub fn meta_get(k: &str) -> Option<String> {
    let db = open_db().ok()?;
    db.query_row("SELECT v FROM meta WHERE k=?1", [k], |r| r.get::<_, String>(0)).ok()
}

pub fn meta_set(k: &str, v: &str) -> Result<(), String> {
    let db = open_db()?;
    db.execute(
        "INSERT INTO meta(k,v) VALUES(?1,?2) ON CONFLICT(k) DO UPDATE SET v=excluded.v",
        [k, v],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
```
测试(仿照 collab 现有单测的 `POLARIS_COLLAB_DB` 临时库模式,与既有测试同风格):
```rust
#[test]
fn meta_roundtrip() {
    let tmp = std::env::temp_dir().join(format!("collab-meta-{}.db", std::process::id()));
    std::env::set_var("POLARIS_COLLAB_DB", &tmp);
    meta_set("host_node_id", "node-abc").unwrap();
    assert_eq!(meta_get("host_node_id").as_deref(), Some("node-abc"));
    meta_set("host_node_id", "node-xyz").unwrap(); // upsert 覆盖
    assert_eq!(meta_get("host_node_id").as_deref(), Some("node-xyz"));
    std::env::remove_var("POLARIS_COLLAB_DB");
    let _ = std::fs::remove_file(&tmp);
}
```
⚠ 若既有测试已有公共的临时库 helper,复用之;env var 是进程级的,该测试需与其他用库测试串行 —— 跟随既有测试的处置方式(它们已用同一模式跑绿 19 个,说明按模块顺序执行没冲突)。

- [ ] **Step 2: 跑测试**
`cargo test --manifest-path src-tauri\Cargo.toml --lib collab::db`
Expected: meta_roundtrip 过。

- [ ] **Step 3: collab_bootstrap 支持 hostSelf**

http.rs `collab_bootstrap` 的 spawn_blocking 闭包里,`login` 之后、`Ok(json!(...))` 之前插:
```rust
// 「把这台电脑设为主机」流程:主机自己的前端在 bootstrap 时自报 hostSelf,
// 顺手把本机登记进设备白名单并落 meta,设备页据此点亮「主机」徽标。
if v.get("hostSelf").and_then(|x| x.as_bool()).unwrap_or(false) {
    let node = s_of(&v, "deviceId");
    if !node.is_empty() {
        let name = std::env::var("COMPUTERNAME")
            .or_else(|_| std::env::var("HOSTNAME"))
            .map(|h| format!("{h}(主机)"))
            .unwrap_or_else(|_| "主机".into());
        let _ = crate::collab::identity::add_device(u.id, &name, &node);
        let _ = crate::collab::db::meta_set("host_node_id", &node);
    }
}
```

- [ ] **Step 4: collab_devices 附 is_host**

http.rs `collab_devices` 中,取到设备列表后改为映射注入(原样返回列表的那行替换):
```rust
let host_node = crate::collab::db::meta_get("host_node_id").unwrap_or_default();
let list = crate::collab::identity::list_devices()?;
let out: Vec<serde_json::Value> = list
    .into_iter()
    .map(|d| {
        let is_host = !host_node.is_empty() && d.node_id == host_node;
        let mut j = serde_json::to_value(&d).unwrap_or_else(|_| serde_json::json!({}));
        if is_host {
            j["is_host"] = serde_json::json!(true);
        }
        j
    })
    .collect();
ok(out)
```
(该 handler 若原本 join 了 username,保持 join 结果为基础做同样的注入 —— 原有字段一个都不许丢。)

- [ ] **Step 5: 双壳编译 + Commit**
```powershell
cargo check --manifest-path src-tauri\Cargo.toml
cargo check --manifest-path src-tauri\Cargo.toml --no-default-features --features server
git add src-tauri/src/collab/db.rs src-tauri/src/collab/http.rs
git commit -m "feat(collab): meta 表落主机标识;bootstrap hostSelf 自登记设备;设备列表附 is_host"
```

---

### Task 5: 桌面内嵌主机运行时 hosting.rs + Tauri 命令

**Files:**
- Create: `src-tauri/src/collab/hosting.rs`
- Modify: `src-tauri/src/collab/mod.rs`
- Modify: `src-tauri/src/lib.rs`(注册命令 + 自启)

- [ ] **Step 1: 写 hosting.rs**

```rust
//! 桌面「把这台电脑设为主机」:进程内嵌 axum,只挂协作端点(/api/collab/*、/git/*、/ws、/api/health)。
//! 不碰 kb/chat 等模块 —— 它们已由桌面 setup 初始化,这里只是给同事开一扇协作门。
#![cfg(feature = "desktop")]

use crate::collab::http::{collab_router, detect_advertise_urls, CollabState};
use crate::host::AppHandle as BusHandle;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

const PORT_SCAN: std::ops::Range<u16> = 8484..8495;

struct Running {
    port: u16,
    urls: Vec<String>,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    bus: BusHandle,
}

static RUNNING: Mutex<Option<Running>> = Mutex::new(None);

#[derive(Serialize, Deserialize, Clone, Copy, Default)]
struct HostCfg {
    enabled: bool,
    port: Option<u16>,
}

fn cfg_path() -> Option<PathBuf> {
    directories::UserDirs::new().map(|u| u.home_dir().join("Polaris/data/collab_host.json"))
}

fn load_cfg() -> HostCfg {
    cfg_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_cfg(c: HostCfg) {
    if let Some(p) = cfg_path() {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(p, serde_json::to_string(&c).unwrap_or_default());
    }
}

/// 内核启动(不依赖 tauri,可单测):绑端口→建独立事件频道→起 axum(graceful shutdown)。
pub async fn start_core(pref_port: Option<u16>) -> Result<(u16, Vec<String>, BusHandle, tokio::sync::oneshot::Sender<()>), String> {
    let (tx, _rx) = tokio::sync::broadcast::channel::<crate::host::Event>(4096);
    let bus = BusHandle::new(tx);
    let auth_token = std::env::var("POLARIS_AUTH_TOKEN").ok().filter(|s| !s.is_empty());

    let mut cands: Vec<u16> = Vec::new();
    if let Some(p) = pref_port {
        cands.push(p);
    }
    cands.extend(PORT_SCAN);
    let mut bound = None;
    for p in cands {
        if let Ok(l) = tokio::net::TcpListener::bind(("0.0.0.0", p)).await {
            bound = Some((l, p));
            break;
        }
    }
    let (listener, port) = bound.ok_or("端口 8484-8494 全被占用,请在设置里换端口")?;
    let urls = detect_advertise_urls(port);

    let state = CollabState {
        app: bus.clone(),
        auth_token: Arc::new(auth_token),
        advertise: Arc::new(parking_lot::RwLock::new(urls.clone())),
    };
    // 分享码探活 + 成员端 REST 都是跨源 → CORS 必开;git push 大包 → body 上限同 server 壳。
    let router = collab_router(state, true)
        .route("/api/health", axum::routing::get(|| async { "ok" }))
        .layer(axum::extract::DefaultBodyLimit::max(512 * 1024 * 1024))
        .layer(tower_http::cors::CorsLayer::permissive());

    let (stx, srx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        let _ = axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = srx.await;
            })
            .await;
    });
    Ok((port, urls, bus, stx))
}

fn status_json() -> serde_json::Value {
    let g = RUNNING.lock();
    let needs_bootstrap = crate::collab::auth::is_bootstrap().unwrap_or(true);
    match g.as_ref() {
        Some(r) => serde_json::json!({
            "running": true, "port": r.port, "urls": r.urls, "needsBootstrap": needs_bootstrap,
            "autostart": load_cfg().enabled,
        }),
        None => serde_json::json!({
            "running": false, "port": 0, "urls": [], "needsBootstrap": needs_bootstrap,
            "autostart": load_cfg().enabled,
        }),
    }
}

/// 事件桥:内嵌服务的广播 → 本机 Tauri UI(主机人自己的看板实时刷新)。
fn bridge_to_ui(app: tauri::AppHandle, bus: &BusHandle) {
    let mut rx = bus.subscribe();
    tauri::async_runtime::spawn(async move {
        use tauri::Emitter;
        while let Ok(ev) = rx.recv().await {
            let _ = app.emit(&ev.topic, ev.payload);
        }
    });
}

#[tauri::command]
pub async fn collab_host_start(app: tauri::AppHandle, port: Option<u16>) -> Result<serde_json::Value, String> {
    if RUNNING.lock().is_some() {
        return Ok(status_json());
    }
    let pref = port.or(load_cfg().port);
    let (port, urls, bus, stx) = start_core(pref).await?;
    bridge_to_ui(app, &bus);
    save_cfg(HostCfg { enabled: true, port: Some(port) });
    *RUNNING.lock() = Some(Running { port, urls, shutdown: Some(stx), bus });
    Ok(status_json())
}

#[tauri::command]
pub fn collab_host_status() -> serde_json::Value {
    status_json()
}

#[tauri::command]
pub fn collab_host_stop() -> Result<serde_json::Value, String> {
    if let Some(mut r) = RUNNING.lock().take() {
        if let Some(s) = r.shutdown.take() {
            let _ = s.send(());
        }
    }
    save_cfg(HostCfg { enabled: false, ..load_cfg() });
    Ok(status_json())
}

/// App 启动时自动拉起(上次开过主机就续上,别让同事早上连不上)。
pub fn auto_start_if_enabled(app: tauri::AppHandle) {
    let cfg = load_cfg();
    if !cfg.enabled {
        return;
    }
    tauri::async_runtime::spawn(async move {
        match start_core(cfg.port).await {
            Ok((port, urls, bus, stx)) => {
                bridge_to_ui(app, &bus);
                *RUNNING.lock() = Some(Running { port, urls, shutdown: Some(stx), bus });
            }
            Err(e) => eprintln!("[collab-host] 自启失败: {e}"),
        }
    });
}

#[cfg(test)]
mod tests {
    #[tokio::test(flavor = "multi_thread")]
    async fn start_core_serves_health_and_bootstrap_gate() {
        let tmp = std::env::temp_dir().join(format!("collab-host-{}.db", std::process::id()));
        std::env::set_var("POLARIS_COLLAB_DB", &tmp);
        let (port, _urls, _bus, stx) = super::start_core(None).await.expect("start");
        // ureq 是既有依赖:同步阻塞客户端,放 spawn_blocking 防塞 runtime
        let body = tokio::task::spawn_blocking(move || {
            ureq::get(&format!("http://127.0.0.1:{port}/api/health"))
                .call()
                .unwrap()
                .into_string()
                .unwrap()
        })
        .await
        .unwrap();
        assert_eq!(body, "ok");
        let _ = stx.send(());
        std::env::remove_var("POLARIS_COLLAB_DB");
        let _ = std::fs::remove_file(&tmp);
    }
}
```

- [ ] **Step 2: mod.rs 挂载**

`src-tauri/src/collab/mod.rs` 加:
```rust
/// 桌面内嵌协作主机(一键当主机)。
#[cfg(feature = "desktop")]
pub mod hosting;
```

- [ ] **Step 3: lib.rs 注册命令 + 自启**

`invoke_handler` 的 collab 段(:151-161)追加三行:
```rust
collab::hosting::collab_host_start,
collab::hosting::collab_host_status,
collab::hosting::collab_host_stop,
```
`.setup` 里 `fable::init();` 之后加:
```rust
// 协作主机自启:上次点过「设为主机」就静默续上(不阻塞启动)。
collab::hosting::auto_start_if_enabled(h.clone());
```

- [ ] **Step 4: 编译 + 单测**
```powershell
cargo check --manifest-path src-tauri\Cargo.toml
cargo test --manifest-path src-tauri\Cargo.toml --lib collab::hosting
```
Expected: check 过;start_core 测试过(health 返回 ok)。

- [ ] **Step 5: Commit**
```powershell
git add src-tauri/src/collab/hosting.rs src-tauri/src/collab/mod.rs src-tauri/src/lib.rs
git commit -m "feat(collab): 桌面内嵌协作主机(一键当主机)+ 自启 + UI 事件桥"
```

---

### Task 6: 前端数据层(api.ts + stores/collab.ts)

**Files:**
- Modify: `src/features/collab/api.ts`
- Modify: `src/features/collab/stores/collab.ts`

- [ ] **Step 1: api.ts — Ticket.share、分享码解析、主机探活**

`Ticket` 接口(:109-113)加 `share?: string;`。`AdminDevice`(:123-130)加 `is_host?: boolean;`。`bootstrap` 的 args 类型加 `hostSelf?: boolean;`(透传即可,POST body 已是 `{...args}` 展开)。文件尾部(fmtTime 附近)追加:

```ts
/** 分享码 PLRS1-<base64url(json{c,a})> → {code, addrs};不是分享码返回 null(裸码走旧流程) */
export function parseShareCode(s: string): { code: string; addrs: string[] } | null {
  const m = s.trim();
  if (!m.startsWith("PLRS1-")) return null;
  try {
    const b64 = m.slice(6).replace(/-/g, "+").replace(/_/g, "/");
    const pad = b64 + "=".repeat((4 - (b64.length % 4)) % 4);
    const v = JSON.parse(atob(pad));
    if (typeof v.c !== "string" || !Array.isArray(v.a)) return null;
    return { code: v.c, addrs: v.a.filter((x: unknown): x is string => typeof x === "string") };
  } catch {
    return null;
  }
}

/** 逐个探活分享码里的地址,返回第一个能 /api/health 通的(2.5s 超时) */
export async function probeHost(addrs: string[]): Promise<string | null> {
  for (const a of addrs) {
    const base = a.replace(/\/+$/, "");
    try {
      const ctl = new AbortController();
      const t = setTimeout(() => ctl.abort(), 2500);
      const r = await fetch(base + "/api/health", { signal: ctl.signal });
      clearTimeout(t);
      if (r.ok) return base;
    } catch {
      /* 试下一个 */
    }
  }
  return null;
}
```
顺手改文件头注释:桌面版已可内嵌协作主机(collab-host),「axum 未编入桌面版」表述已过时。

- [ ] **Step 2: stores/collab.ts — 主机状态三动作 + redeem 分享码 + bootstrap hostSelf**

顶部 import 区把 `import { isTauri, listen } from "../../../tauri";` 改为 `import { invoke, isTauri, listen } from "../../../tauri";`,并从 `../api` 追加导入 `parseShareCode, probeHost`。

state 区(`needsHost` 附近)加:
```ts
export interface HostInfo {
  running: boolean;
  port: number;
  urls: string[];
  needsBootstrap: boolean;
  autostart: boolean;
}
```
(接口放 store 文件顶层,defineStore 外。)store 内加:
```ts
const hostInfo = ref<HostInfo | null>(null);

/** 桌面内嵌主机:状态/启动/停止(非桌面环境静默 no-op) */
async function hostStatus(): Promise<HostInfo | null> {
  if (!isTauri) return null;
  try {
    hostInfo.value = await invoke<HostInfo>("collab_host_status");
  } catch {
    hostInfo.value = null;
  }
  return hostInfo.value;
}

async function hostStart(): Promise<HostInfo> {
  const info = await invoke<HostInfo>("collab_host_start");
  hostInfo.value = info;
  applyBase(`http://127.0.0.1:${info.port}`);
  return info;
}

async function hostStop() {
  await invoke<HostInfo>("collab_host_stop");
  await hostStatus();
}
```
`bootstrap` 签名加尾参:
```ts
async function bootstrap(
  username: string,
  password: string,
  displayName: string,
  hostSelf = false
) {
  const r = requireAuth(
    await collabApi.bootstrap({ username, password, displayName, hostSelf })
  );
  persistSession(r.user, r.token);
  await afterAuth();
}
```
`redeem` 改为先解析分享码(旧裸码路径不动):
```ts
async function redeem(args: {
  code: string;
  username: string;
  password: string;
  displayName: string;
  deviceName: string;
}) {
  let code = args.code.trim();
  const parsed = parseShareCode(code);
  if (parsed) {
    const found = await probeHost(parsed.addrs);
    if (!found) {
      throw new Error("配对码里的主机地址都连不上 —— 确认主机开着,且你们在同一网络/VPN 里");
    }
    applyBase(found);
    code = parsed.code;
  }
  const r = requireAuth(
    await collabApi.redeem({ ...args, code, nodeId: deviceId() })
  );
  persistSession(r.user, r.token);
  await afterAuth();
}
```
return 导出对象追加 `hostInfo, hostStatus, hostStart, hostStop`。

- [ ] **Step 3: 类型检查**
`npx vue-tsc --noEmit`
Expected: exit 0。

- [ ] **Step 4: Commit**
```powershell
git add src/features/collab/api.ts src/features/collab/stores/collab.ts
git commit -m "feat(collab-fe): 分享码解析+主机探活+内嵌主机三动作+bootstrap hostSelf"
```

---

### Task 7: 前端 UI(CollabView 一键当主机 / redeem 免地址 / Admin 分享码+徽标+主机卡)

**Files:**
- Modify: `src/features/collab/CollabView.vue`
- Modify: `src/features/collab/CollabAdmin.vue`

- [ ] **Step 1: CollabView — 一键当主机 CTA**

script 区加:
```ts
const hostBusy = ref(false);
async function makeHost() {
  hostBusy.value = true;
  try {
    const info = await collab.hostStart();
    tab.value = info.needsBootstrap ? "bootstrap" : "login";
    toast.info(
      info.needsBootstrap
        ? `主机已在本机启动(端口 ${info.port}),注册你的管理者账号吧`
        : `主机已在本机启动(端口 ${info.port}),直接登录`
    );
  } catch (e) {
    toast.error((e as Error).message);
  } finally {
    hostBusy.value = false;
  }
}
```
`onMounted` 改为:
```ts
onMounted(() => {
  void collab.init();
  if (isTauri) {
    void collab.hostStatus().then((s) => {
      // 主机自启续联:本机在当主机但 base 丢了(如清过缓存)→ 自动指回本机
      if (s?.running && !collab.base) collab.applyBase(`http://127.0.0.1:${s.port}`);
    });
  }
});
```
模板:auth-card 里 `auth-lead` 那行(`:213`)之后、tabs 之前插:
```html
<div v-if="isTauri" class="host-cta">
  <template v-if="!collab.hostInfo?.running">
    <button class="btn solid wide" :disabled="hostBusy" @click="makeHost">
      <LoaderCircle v-if="hostBusy" :size="14" class="spin" />
      把这台电脑设为主机
    </button>
    <p class="cta-tip">本机启动协作服务,注册管理者账号;同事凭一个配对码加入,谁都不用填地址。</p>
  </template>
  <p v-else class="cta-on">本机主机运行中 · 端口 {{ collab.hostInfo.port }}</p>
  <div class="cta-divider"><span>或者连接别人的主机</span></div>
</div>
```
样式(style 区追加):
```css
.host-cta { margin-bottom: 14px; }
.cta-tip { font-size: 12px; color: var(--muted); margin: 8px 0 0; line-height: 1.6; }
.cta-on { font-size: 12.5px; color: var(--primary, var(--ink)); font-weight: 600; margin: 0; }
.cta-divider { display: flex; align-items: center; gap: 10px; margin-top: 14px; color: var(--muted); font-size: 11px; }
.cta-divider::before, .cta-divider::after { content: ""; flex: 1; height: 1px; background: var(--line, currentColor); opacity: .25; }
```

- [ ] **Step 2: CollabView — redeem 免地址(放行分享码)**

`doAuth` 的 needsHost 拦截(:85-89)改为分享码豁免:
```ts
// 桌面版没连主机就登录 → 请求会落到应用自己身上 → 天书报错。分享码自带地址,豁免。
const shareOk = tab.value === "redeem" && !!parseShareCode(f.value.code);
if (collab.needsHost && !shareOk) {
  authErr.value = "桌面版请先在下方「高级设置」填写协作主机地址,或让主机管理者发你配对码";
  hostOpen.value = true;
  return;
}
```
script 顶部从 `./api` 导入 `parseShareCode`。bootstrap 分支传 hostSelf:
```ts
} else if (tab.value === "bootstrap") {
  await collab.bootstrap(
    v.username.trim(), v.password, v.displayName.trim(),
    !!collab.hostInfo?.running // 本机正在当主机 → 自报主机设备,设备页点亮徽标
  );
}
```
redeem 输入框 placeholder 改为 `"邀请配对码(粘贴整串)"`。

- [ ] **Step 3: CollabAdmin — 分享码展示 + 设备主机徽标 + 本机主机卡**

票据展示块(`.ticket` 内),`.tk-code` 之后加分享码行,复制按钮改复制分享码:
```html
<div v-if="ticket.share" class="tk-share">{{ ticket.share }}</div>
```
`copyCode()` 改:
```ts
await navigator.clipboard.writeText(ticket.value.share || ticket.value.code);
```
样式:
```css
.tk-share { font-family: var(--mono); font-size: 11px; color: var(--muted); word-break: break-all; margin-top: 6px; user-select: all; line-height: 1.5; }
```
设备表第一列加徽标(:175 附近):
```html
<td>
  <span v-if="d.is_host" class="badge-host">主机</span>
  {{ d.name || d.node_id || d.id }}
</td>
```
```css
.badge-host { font-size: 10px; font-weight: 700; color: #b8860b; background: color-mix(in srgb, #b8860b 14%, transparent); border-radius: 4px; padding: 1px 6px; margin-right: 6px; vertical-align: 1px; }
```
页面顶部(生成邀请票据卡之前)加本机主机卡(仅桌面显示):
```html
<section v-if="isTauri && collab.hostInfo?.running" class="card">
  <h3><Server :size="15" /> 本机主机</h3>
  <div class="row">
    <span class="dim">端口 {{ collab.hostInfo.port }}</span>
    <span v-for="u in collab.hostInfo.urls" :key="u" class="mono dim">{{ u }}</span>
    <button class="btn danger sm" @click="stopHost">停止主机</button>
  </div>
  <p class="dim" style="font-size:11px">停止后同事将连不上;下次启动 App 不再自动开启。</p>
</section>
```
script:导入 `isTauri`(from `../../tauri`)、lucide `Server`,加:
```ts
async function stopHost() {
  if (!confirm("停止本机协作主机?同事将立即连不上。")) return;
  try {
    await collab.hostStop();
    toast.info("主机已停止");
  } catch (e) {
    toast.error((e as Error).message);
  }
}
```
onMounted 里补 `void collab.hostStatus();`。

- [ ] **Step 4: 类型检查 + 前端构建**
```powershell
npx vue-tsc --noEmit
npm run build
```
Expected: 双绿(桌面改前端必须 build,嵌入式 dist)。

- [ ] **Step 5: Commit**
```powershell
git add src/features/collab/CollabView.vue src/features/collab/CollabAdmin.vue
git commit -m "feat(collab-fe): 一键当主机 CTA + 分享码免地址入伙 + 设备主机徽标 + 本机主机卡"
```

---

### Task 8: 端到端验证(真机链路)

**Files:** 无新改动;发现问题回上面的任务修。

- [ ] **Step 1: 重建并启动桌面 App**
```powershell
Get-Process polaris-app,polaris-server -ErrorAction SilentlyContinue | Stop-Process -Force
npm run tauri:dev   # 后台跑,等窗口起来
```

- [ ] **Step 2: 主机链路(用户或驱动器点验)**
1. 协作页 → 「把这台电脑设为主机」→ 应跳到初始化 tab,toast 报端口。
2. 注册 owner 账号 → 进入看板。
3. 管理页 → 生成邀请票据 → 出现 `PLRS1-...` 分享码 → 复制。
4. 设备页 → 本机行带「主机」徽标。

- [ ] **Step 3: 成员链路(免开第二台机,用 curl 模拟)**
```powershell
# 解开分享码(取 code 与 addrs 第一个),模拟同事兑换:
curl.exe -s -X POST "<addr>/api/collab/redeem" -H "Content-Type: application/json" -d '{"code":"<裸码>","username":"bob","password":"pass1234","displayName":"Bob","deviceName":"bob-pc","nodeId":"node-bob-1"}'
```
Expected: 返回 user+token;桌面设备页刷新出现 bob-pc(无主机徽标)。

- [ ] **Step 4: 重启持久化验证**
关掉桌面 App 重开 → 不点任何按钮,管理页「本机主机」卡应显示运行中(自启生效),成员 token 仍可 `GET /api/collab/me`。

- [ ] **Step 5: 回归**
```powershell
cargo test --manifest-path src-tauri\Cargo.toml --no-default-features --features server
npx vue-tsc --noEmit
```
Expected: 全绿。至此 ①②③ 交付完毕。

---

## Self-Review 结论(已核)

- **覆盖:** ①一键当主机=Task 1/2/5/7;②配对码带地址=Task 3/6/7;③主机徽标=Task 4/7;持久化自启=Task 5;E2E=Task 8。
- **一致性:** `CollabState{app,auth_token,advertise}`、`collab_router(state, with_ws)`、`HostInfo{running,port,urls,needsBootstrap,autostart}`、命令名 `collab_host_start/status/stop`、分享码前缀 `PLRS1-`、meta 键 `host_node_id` —— 各任务间已对齐。
- **已知取舍:** 换主(mirror 迁移)不在本计划;`collab_host_set_autostart` 独立开关未做(停止主机即关自启,够用);tunnel(collab-net)默认不编译,分享码走 LAN/Tailscale 地址已覆盖目标场景。

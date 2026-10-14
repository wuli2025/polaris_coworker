use super::*;

// ───────────────────────── /api/status 运维水位(R7)─────────────────────────
//
// 给群晖运维/监控用的水位接口: 容器内存(贴近 mem_limit/OOM 风险)、宿主内存、数据盘用量
// (防写满)、claude 配置在位、推理端点(R3)状态。全部 best-effort: 读不到的项返回
// available:false 而非报错, 非 Linux 环境(开发机)也能编译运行。详细状态会暴露端点和
// 宿主资源信息，因此仅 owner 可读；结果短时缓存且单航班采集，避免并发耗尽阻塞线程池。

static STATUS_CACHE: Lazy<tokio::sync::Mutex<Option<(Instant, Value)>>> =
    Lazy::new(|| tokio::sync::Mutex::new(None));

pub(super) async fn status(
    State(state): State<AppState>,
    peer: Option<axum::extract::ConnectInfo<std::net::SocketAddr>>,
    headers: HeaderMap,
) -> Response {
    // 鉴权复用 apihub 的基础面解析(与 /api/invoke 同语义);role_rank 在 collab::http。
    let gate = crate::apihub::server_origin_gate(state.open_no_auth, peer.map(|c| c.0), &headers);
    let Some(ctx) = crate::apihub::app_ctx_headers(&state.auth_token, &headers, gate) else {
        return (StatusCode::UNAUTHORIZED, Json(json!({"error":"未授权"}))).into_response();
    };
    if crate::collab::http::role_rank(&ctx.role) < 3 {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error":"状态详情仅主机管理员可读"})),
        )
            .into_response();
    }
    // 口令可能来自环境变量,也可能是用户在首次访问向导里自己设的 —— 运维水位要如实反映。
    let auth_set = crate::apihub::effective_token(&state.auth_token).is_some();
    let init_errors = state.readiness.init_errors.clone();
    let mut cache = STATUS_CACHE.lock().await;
    if let Some((at, value)) = cache.as_ref() {
        if at.elapsed() < Duration::from_secs(15) {
            return Json(value.clone()).into_response();
        }
    }
    // 含 df 子进程 + 推理端点探测(阻塞/网络), 丢到阻塞线程池, 勿卡 async worker。
    let v = tokio::task::spawn_blocking(move || collect_status(auth_set, init_errors))
        .await
        .unwrap_or_else(|_| json!({ "ok": false, "error": "status 采集失败" }));
    *cache = Some((Instant::now(), v.clone()));
    Json(v).into_response()
}

fn collect_status(auth_set: bool, init_errors: Vec<String>) -> Value {
    let data_root = directories::UserDirs::new()
        .map(|u| u.home_dir().join("Polaris"))
        .unwrap_or_else(|| PathBuf::from("/root/Polaris"));
    json!({
        "ok": true,
        "service": "polaris-server",
        "auth_token_set": auth_set,
        "startup": {
            "ready": init_errors.is_empty(),
            "errors": init_errors,
        },
        "chat_timeout_secs": std::env::var("POLARIS_CHAT_TIMEOUT_SECS")
            .ok().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0),
        "container_memory": cgroup_mem(),
        "host_memory": meminfo_mem(),
        "data_disk": disk_usage(&data_root),
        "agent_runtime": crate::chat::backend::agent_backend_status().map_or_else(
            |_| json!({"healthy":false,"error":"AgentConfigInvalid"}),
            |status| json!(status)),
        "infer": crate::infer::status_json(),
        "forge": crate::forge::forge_preflight(),
    })
}

fn pct(used: u64, total: u64) -> Option<f64> {
    if total == 0 {
        None
    } else {
        Some(((used as f64 / total as f64) * 1000.0).round() / 10.0)
    }
}

/// cgroup v2 容器内存(比宿主内存更贴近 mem_limit / OOM 风险)。
fn cgroup_mem() -> Value {
    let used = std::fs::read_to_string("/sys/fs/cgroup/memory.current")
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok());
    // memory.max 为 "max" 表示未设上限 → parse 失败即视为无上限。
    let limit = std::fs::read_to_string("/sys/fs/cgroup/memory.max")
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok());
    match (used, limit) {
        (Some(u), Some(l)) => json!({ "used_bytes": u, "limit_bytes": l, "used_pct": pct(u, l) }),
        (Some(u), None) => json!({
            "used_bytes": u, "limit_bytes": null, "used_pct": null,
            "note": "未设容器内存上限(memory.max=max)，建议设 mem_limit 防泄漏拖垮整机"
        }),
        _ => json!({ "available": false, "note": "非 cgroup v2 环境或无权读取" }),
    }
}

/// 宿主可用内存(/proc/meminfo)。
fn meminfo_mem() -> Value {
    let Ok(txt) = std::fs::read_to_string("/proc/meminfo") else {
        return json!({ "available": false });
    };
    let kb_to_bytes = |line: &str, key: &str| -> Option<u64> {
        line.strip_prefix(key)
            .and_then(|r| r.trim().trim_end_matches("kB").trim().parse::<u64>().ok())
            .map(|k| k * 1024)
    };
    let mut total = None;
    let mut avail = None;
    for line in txt.lines() {
        if total.is_none() {
            if let Some(b) = kb_to_bytes(line, "MemTotal:") {
                total = Some(b);
            }
        }
        if avail.is_none() {
            if let Some(b) = kb_to_bytes(line, "MemAvailable:") {
                avail = Some(b);
            }
        }
    }
    match (total, avail) {
        (Some(t), Some(a)) => json!({
            "total_bytes": t, "available_bytes": a, "used_pct": pct(t.saturating_sub(a), t)
        }),
        _ => json!({ "available": false }),
    }
}

/// 数据盘用量(df -kP <path>)。防「容器写满 /volume1 卷拖垮 DSM」的水位来源。
fn disk_usage(path: &Path) -> Value {
    let Ok(out) = std::process::Command::new("df")
        .arg("-kP")
        .arg(path)
        .output()
    else {
        return json!({ "available": false, "note": "df 不可用" });
    };
    if !out.status.success() {
        return json!({ "available": false });
    }
    let txt = String::from_utf8_lossy(&out.stdout);
    if let Some(line) = txt.lines().nth(1) {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() >= 4 {
            let total = f[1].parse::<u64>().ok().map(|k| k * 1024);
            let used = f[2].parse::<u64>().ok().map(|k| k * 1024);
            let avail = f[3].parse::<u64>().ok().map(|k| k * 1024);
            if let (Some(t), Some(u), Some(a)) = (total, used, avail) {
                return json!({
                    "path": path.to_string_lossy(),
                    "total_bytes": t, "used_bytes": u, "available_bytes": a,
                    "used_pct": pct(u, t)
                });
            }
        }
    }
    json!({ "available": false })
}


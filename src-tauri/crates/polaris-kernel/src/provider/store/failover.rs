use super::*;

// ─────────────────── 故障转移(polaris-failover 滑窗) ───────────────────
// 5 分钟窗口内同一供应商第 3 次网络/鉴权/欠费类失败 → 自动切到列表里下一个可用
// 供应商, 并 emit 事件让前端 toast。不自动切回(防抖动), 切回由用户在坞里确认。
pub(super) static FAILOVER: Lazy<polaris_failover::FailoverTracker> =
    Lazy::new(polaris_failover::FailoverTracker::new);
/// init() 时存一份 AppHandle 供故障转移线程 emit 事件用 —— 失败上报点(本地路由
/// codex_proxy 的 TCP 线程)手里没有 app 句柄, 只能走全局。
pub(super) static FAILOVER_APP: Lazy<RwLock<Option<AppHandle>>> = Lazy::new(|| RwLock::new(None));

/// 当前激活供应商 id(本地路由无 `/p/{id}` 前缀的旧版路径需要它做故障归因)。
pub fn current_provider_id() -> String {
    STORE.read().current_id.clone()
}

/// 请求成功: 清零该供应商的失败计数(偶发抖动不积累)。
pub fn note_provider_success(id: &str) {
    FAILOVER.record_success(id);
}

/// 记一次网络/鉴权/欠费类失败; 达到滑窗阈值时自动切换到下一个可用供应商。
/// 取舍说明: 最理想的上报点是「claude 子进程每次上游请求的结果」, 但直连模式下
/// 请求发生在 claude CLI 内部, kernel 看不见 —— 所以接在**本地路由**(codex_proxy,
/// cc-switch 式代理, 每请求过手)的错误分类点上: 透传 4xx/5xx、OpenAI 兼容 401、
/// ChatGPT 刷新失败、连接超时等都会走到这里。直连供应商享受不到自动转移, 属已知边界。
pub fn note_provider_failure(id: &str) {
    if !FAILOVER.record_failure(id) {
        return; // 未达阈值: 只计数
    }
    // 达阈: 挑「当前供应商列表里失败者之后的下一个可用项」(环形), 沿用 provider_switch
    // 完成真正切换(env/settings 写入逻辑零重复)。仅当失败者就是当前供应商才切 ——
    // 用户手动在用别家时, 后台某路由请求失败不该抢方向盘。
    let (views, current_id) = {
        let store = STORE.read().clone();
        (build_views(&store), store.current_id.clone())
    };
    if current_id != id {
        return;
    }
    let Some(pos) = views.iter().position(|v| v.id == id) else {
        return;
    };
    let failed_name = views[pos].name.clone();
    // 环形找下一个「有 key 且不是失败者」的供应商; 找不到备用就只广播失败, 不切。
    let next = (1..views.len())
        .map(|off| &views[(pos + off) % views.len()])
        .find(|v| v.has_key && v.id != id)
        .cloned();
    let Some(next) = next else {
        eprintln!("[provider-failover] {failed_name} 连续失败, 但没有其他可用供应商, 不切换");
        return;
    };
    match provider_switch(next.id.clone()) {
        Ok(_) => {
            let msg = format!("供应商「{failed_name}」连续失败, 已自动切换到「{}」", next.name);
            eprintln!("[provider-failover] {msg}");
            emit_failover(&json!({
                "kind": "switched",
                "from": id,
                "fromName": failed_name,
                "to": next.id,
                "toName": next.name,
                "message": msg,
            }));
        }
        Err(e) => eprintln!("[provider-failover] 自动切换失败: {e}"),
    }
}

pub(super) fn emit_failover(payload: &Value) {
    #[cfg(feature = "desktop")]
    {
        use tauri::Emitter;
        if let Some(app) = FAILOVER_APP.read().as_ref() {
            let _ = app.emit("provider://failover", payload.clone());
        }
    }
    #[cfg(not(feature = "desktop"))]
    {
        if let Some(app) = FAILOVER_APP.read().as_ref() {
            let _ = app.emit("provider://failover", payload.clone());
        }
    }
}


/// Context overflow, local tools, cancellation and daemon faults are not provider outages.
pub fn pi_failure_counts(code: &str) -> bool {
    matches!(code, "MODEL_AUTH" | "MODEL_RATE_LIMIT" | "MODEL_NETWORK" | "MODEL_ERROR")
}
pub fn pi_outcome(provider: &str, success: bool, failed: bool, code: Option<&str>) {
    let id = pi_translate::polaris_id(provider);
    if success { note_provider_success(id); }
    else if failed && code.is_some_and(pi_failure_counts) { note_provider_failure(id); }
}

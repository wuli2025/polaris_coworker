#[cfg(not(feature = "desktop"))]
fn update_state_dir() -> PathBuf {
    directories::UserDirs::new()
        .map(|u| u.home_dir().join("Polaris").join("data").join("updates"))
        .unwrap_or_else(|| PathBuf::from("/home/polaris/Polaris/data/updates"))
}

#[cfg(not(feature = "desktop"))]
fn atomic_write_json(path: &Path, value: &Value) -> Result<(), String> {
    let parent = path.parent().ok_or("更新状态路径没有父目录")?;
    std::fs::create_dir_all(parent).map_err(|e| format!("创建更新状态目录失败:{e}"))?;
    let tmp = parent.join(format!(
        ".{}.tmp-{}",
        path.file_name().and_then(|v| v.to_str()).unwrap_or("state"),
        std::process::id()
    ));
    let bytes = serde_json::to_vec(value).map_err(|e| format!("序列化更新状态失败:{e}"))?;
    std::fs::write(&tmp, bytes).map_err(|e| format!("写更新状态失败:{e}"))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("提交更新状态失败:{e}"))
}

#[cfg(not(feature = "desktop"))]
fn read_json_file(path: &Path) -> Option<Value> {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
}

#[cfg(not(feature = "desktop"))]
fn update_request_id() -> String {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("{now}-{seq}")
}

#[cfg(not(feature = "desktop"))]
fn valid_update_request_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 64 && value.chars().all(|c| c.is_ascii_digit() || c == '-')
}

#[cfg(not(feature = "desktop"))]
fn derive_docker_update_state(
    request: &Value,
    result: Option<&Value>,
    identity: &Value,
    now: u64,
) -> &'static str {
    let target = request
        .get("targetRevision")
        .and_then(Value::as_str)
        .unwrap_or("");
    let original_boot = request
        .get("sourceBootId")
        .and_then(Value::as_str)
        .unwrap_or("");
    let current_revision = identity
        .get("buildRevision")
        .and_then(Value::as_str)
        .unwrap_or("");
    let current_boot = identity.get("bootId").and_then(Value::as_str).unwrap_or("");
    let deadline = request.get("deadline").and_then(Value::as_u64).unwrap_or(0);
    let script_state = result
        .and_then(|v| v.get("state"))
        .and_then(Value::as_str)
        .unwrap_or("");

    if !target.is_empty()
        && current_revision == target
        && !original_boot.is_empty()
        && current_boot != original_boot
    {
        "succeeded"
    } else if script_state == "failed" {
        "failed"
    } else if deadline > 0 && now >= deadline {
        "unconfirmed"
    } else if script_state == "watchtower_returned" {
        "waiting_restart"
    } else {
        "triggering"
    }
}

#[cfg(not(feature = "desktop"))]
fn docker_update_status(request_id: &str) -> Result<Value, String> {
    if !valid_update_request_id(request_id) {
        return Err("更新 requestId 无效".into());
    }
    let dir = update_state_dir();
    let request_path = dir.join(format!("{request_id}.json"));
    let mut request = read_json_file(&request_path).ok_or("找不到这次更新请求")?;
    let result = read_json_file(&dir.join(format!("{request_id}.result.json")));
    let identity = docker_build_identity();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let state = derive_docker_update_state(&request, result.as_ref(), &identity, now);
    request["state"] = json!(state);
    request["currentBuild"] = json!(identity);
    request["triggerResult"] = result.unwrap_or(Value::Null);
    Ok(request)
}

#[cfg(not(feature = "desktop"))]
fn docker_update_deadline_secs() -> u64 {
    if std::env::var("POLARIS_UPDATE_E2E").ok().as_deref() == Some("1") {
        return nonempty_env("POLARIS_UPDATE_DEADLINE_SECONDS")
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(15 * 60)
            .clamp(10, 15 * 60);
    }
    15 * 60
}

#[cfg(not(feature = "desktop"))]
fn spawn_docker_update(target: &RegistryTarget) -> Result<Value, String> {
    let request_id = update_request_id();
    let dir = update_state_dir();
    let request_path = dir.join(format!("{request_id}.json"));
    let result_path = dir.join(format!("{request_id}.result.json"));
    let identity = docker_build_identity();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let deadline = now.saturating_add(docker_update_deadline_secs());
    let request = json!({
        "requestId": request_id,
        "state": "queued",
        "requestedAt": now,
        "deadline": deadline,
        "sourceBootId": identity.get("bootId").cloned().unwrap_or(Value::Null),
        "sourceRevision": current_build_revision(),
        "targetRevision": target.revision,
        "targetVersion": target.version,
        "targetDigest": target.index_digest,
        "image": target.image,
    });
    atomic_write_json(&request_path, &request)?;

    let mut child = std::process::Command::new(UPDATE_SCRIPT)
        .arg("--trigger")
        .env("POLARIS_UPDATE_RESULT_FILE", &result_path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("启动后台更新触发器失败:{e}"))?;
    let result_for_waiter = result_path.clone();
    if let Err(e) = std::thread::Builder::new()
        .name("docker-update-trigger".into())
        .spawn(move || {
            let waited = child.wait();
            if result_for_waiter.exists() {
                return;
            }
            let (code, message) = match waited {
                Ok(status) => (
                    status.code(),
                    format!("更新触发器退出，但没有留下结果（status={status}）"),
                ),
                Err(e) => (None, format!("等待更新触发器失败:{e}")),
            };
            let _ = atomic_write_json(
                &result_for_waiter,
                &json!({"state":"failed", "exitCode":code, "message":message}),
            );
        })
    {
        // 子进程已经成功 handoff；此时再因“回收线程起不来”返回失败会制造假阴性。
        // 没有 waiter 的最坏结果是脚本未留结果时到 deadline 进入 unconfirmed，仍是有限状态。
        eprintln!("[docker-update] 回收线程启动失败，更新触发器仍在运行:{e}");
    }

    Ok(json!({
        "accepted": true,
        "requestId": request_id,
        "targetRevision": target.revision,
        "targetVersion": target.version,
        "targetDigest": target.index_digest,
        "sourceBootId": identity.get("bootId").cloned().unwrap_or(Value::Null),
        "deadline": deadline,
        "note": "更新请求已在本机排队；页面会等待容器重启并核对目标 build，不会把 Watchtower 的空 200 当成成功。",
    }))
}

/// 远程更新会触发宿主机侧容器替换。只有部署者已经显式打开机器口令或团队账号登录时，
/// 才允许把这项能力暴露给 Web；开放模式即使误配了 updater token 也 fail-closed。
#[cfg(not(feature = "desktop"))]
fn docker_update_auth_configured_with(token: Option<&str>, require_login: bool) -> bool {
    token.is_some_and(|v| !v.trim().is_empty()) || require_login
}

#[cfg(not(feature = "desktop"))]
fn docker_update_auth_configured() -> bool {
    let token = std::env::var("POLARIS_AUTH_TOKEN").ok();
    docker_update_auth_configured_with(token.as_deref(), require_login_env())
}

/// App 只认一个带 Bearer token 的 Watchtower HTTP API，不探测、更不接触 docker.sock。
/// URL 与 token 都必须显式存在；URL 钉死在 Compose 内网服务名，避免一枚更新密钥被误发
/// 到部署者可控的其他主机。
#[cfg(not(feature = "desktop"))]
fn updater_service_configured_with(url: Option<&str>, token: Option<&str>) -> bool {
    let Some(url) = url.map(str::trim).filter(|v| !v.is_empty()) else {
        return false;
    };
    url == UPDATE_SERVICE_URL && token.is_some_and(|v| !v.trim().is_empty())
}

#[cfg(not(feature = "desktop"))]
fn updater_service_configured() -> bool {
    let url = std::env::var("POLARIS_UPDATER_URL").ok();
    let token = std::env::var("POLARIS_UPDATER_TOKEN").ok();
    updater_service_configured_with(url.as_deref(), token.as_deref())
}

/// 「网页上能不能一键更新」= 已启用应用鉴权 + 隔离 updater 已配置 + 更新脚本在镜像里。
/// 返回 `(enabled, updater_service_configured, script_present)`。
#[cfg(not(feature = "desktop"))]
pub(crate) fn docker_updater_bits() -> (bool, bool, bool) {
    let service = updater_service_configured();
    let script = std::path::Path::new(UPDATE_SCRIPT).exists();
    (
        service && script && docker_update_auth_configured(),
        service,
        script,
    )
}

/// 读取运行 tag 的 OCI index/config，并以 build revision 判断是否真的有可拉取的新镜像。
/// main 上即使 semver 没变，只要 `latest` revision 变了也能正确提示；反过来 release JSON
/// 变了但固定 tag 没变时，不再制造“有更新”假象。
#[cfg(not(feature = "desktop"))]
fn docker_check_update() -> Value {
    match docker_registry_target() {
        Ok(target) => {
            let current_revision = current_build_revision();
            json!({
                "ok": true,
                "current": current_build_version(),
                "latest": target.version,
                "current_revision": current_revision,
                "target_revision": target.revision,
                "target_digest": target.index_digest,
                "has_update": current_revision != target.revision,
                "image": target.image,
                "source": "oci-registry",
            })
        }
        Err(error) => json!({
            "ok": false,
            "current": current_build_version(),
            "current_revision": current_build_revision(),
            "has_update": false,
            "error": error,
        }),
    }
}

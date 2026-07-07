//! collab/commands.rs —— 桌面端(完整端)本地 tauri 命令。
//!
//! 完整端拓扑:看板/账号等走 HTTP 到主机(前端 collab api 已带 base+token);
//! 但 git 工作集操作发生在**成员本机**——这里把 workset 六件套暴露给前端 invoke。
//! 隧道客户端(空壳/完整端连主机)也在此启停。
#![cfg(feature = "desktop")]

use super::workset;

#[tauri::command]
pub fn collab_clone_partial(remoteUrl: String, dest: String, sparseDirs: Vec<String>) -> Result<(), String> {
    let dirs: Vec<&str> = sparseDirs.iter().map(|s| s.as_str()).collect();
    workset::clone_partial(&remoteUrl, std::path::Path::new(&dest), &dirs)
}

#[tauri::command]
pub fn collab_task_setup(repo: String, branch: String, scope: String) -> Result<workset::SetupReport, String> {
    workset::task_setup(std::path::Path::new(&repo), &branch, &scope)
}

#[tauri::command]
pub fn collab_sync_main(repo: String) -> Result<String, String> {
    workset::sync_main(std::path::Path::new(&repo))
}

/// 推分支。带 scope 时先做越界照妖镜:改动落在 scope 外且未 confirm → 拒推并列出文件,
/// 前端二段确认后带 confirm=true 再推(软约束,主机检查闸另有硬闸)。
#[tauri::command]
pub fn collab_push_branch(
    repo: String,
    branch: String,
    scope: Option<String>,
    confirm: Option<bool>,
) -> Result<(), String> {
    let repo = std::path::Path::new(&repo);
    if let Some(scope) = scope.as_deref().filter(|s| !s.trim().is_empty()) {
        if !confirm.unwrap_or(false) {
            let outside = workset::out_of_scope_files(repo, scope)?;
            if !outside.is_empty() {
                return Err(format!(
                    "OUT_OF_SCOPE:改动越出任务地盘(scope):{}。确认要推吗?",
                    outside.join(", ")
                ));
            }
        }
    }
    workset::push_branch(repo, &branch)
}

/// scope 就位状态:本地稀疏集 vs 任务 scope,前端展示「已就位/缺失」。
#[tauri::command]
pub fn collab_scope_status(repo: String, scope: String) -> Result<workset::ScopeStatus, String> {
    workset::scope_status(std::path::Path::new(&repo), &scope)
}

/// 把 outbox 积压消息补传到主机(断线缓存的发送端,连上/重连后调)。
#[tauri::command]
pub fn collab_outbox_flush(dir: String, baseUrl: String, token: String) -> Result<workset::FlushReport, String> {
    workset::flush_outbox(std::path::Path::new(&dir), &baseUrl, &token)
}

/// outbox 目录:前端传空串 = 默认 ~/Polaris/data/outbox(前端无从得知 home,别让它猜)。
fn outbox_dir(dir: &str) -> Result<std::path::PathBuf, String> {
    if !dir.trim().is_empty() {
        return Ok(std::path::PathBuf::from(dir));
    }
    directories::UserDirs::new()
        .map(|u| u.home_dir().join("Polaris").join("data").join("outbox"))
        .ok_or_else(|| "无法定位用户目录".into())
}

#[tauri::command]
pub fn collab_outbox_queue(dir: String, payload: String) -> Result<String, String> {
    workset::queue_message(&outbox_dir(&dir)?, &payload)
}

#[tauri::command]
pub fn collab_outbox_pending(dir: String) -> Result<Vec<(String, String)>, String> {
    workset::pending_messages(&outbox_dir(&dir)?)
}

#[tauri::command]
pub fn collab_outbox_mark_sent(dir: String, idemKey: String) -> Result<(), String> {
    workset::mark_sent(&outbox_dir(&dir)?, &idemKey)
}

/// 成员端设备 NodeId(入伙页展示/上报,主机据此加白名单)。未编 collab-net 时给出说明。
#[tauri::command]
pub fn collab_device_node_id() -> Result<String, String> {
    #[cfg(feature = "collab-net")]
    {
        return super::tunnel::node_id_of_device_key();
    }
    #[cfg(not(feature = "collab-net"))]
    Err("本构建未启用 collab-net(iroh) 功能,直连模式请直接填主机地址".into())
}

/// 成员端隧道:本地端口 ↔ 主机 NodeId。起来后前端把 collab base 设为 http://127.0.0.1:<port>。
#[tauri::command]
pub fn collab_tunnel_connect(hostNodeId: String, listenPort: u16) -> Result<(), String> {
    #[cfg(feature = "collab-net")]
    {
        super::tunnel::start_client_blocking_thread(hostNodeId, listenPort);
        return Ok(());
    }
    #[cfg(not(feature = "collab-net"))]
    {
        let _ = (hostNodeId, listenPort);
        Err("本构建未启用 collab-net(iroh) 功能".into())
    }
}

#[tauri::command]
pub fn collab_tunnel_status() -> serde_json::Value {
    #[cfg(feature = "collab-net")]
    {
        return super::tunnel::status();
    }
    #[cfg(not(feature = "collab-net"))]
    serde_json::json!({"running": false, "unavailable": "本构建未启用 collab-net"})
}

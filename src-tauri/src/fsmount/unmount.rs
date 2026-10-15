//! User-requested teardown and internal mesh cleanup.
use super::*;

/// 卸载一块远程盘(拆桥 + 删盘符)。幂等。
#[cfg_attr(feature = "desktop", tauri::command)]
#[allow(non_snake_case)]
pub async fn fs_unmount(sourceId: String) -> Result<(), String> {
    crate::mesh::require_local("remove_remote_storage", &json!({"sourceId":sourceId}))?;
    unmount_authorized(sourceId).await
}

pub(crate) async fn unmount_authorized(source_id: String) -> Result<(), String> {
    let m = mounts().lock().unwrap_or_else(|error| error.into_inner()).remove(&source_id);
    if let Some(m) = m {
        m.alive.store(false, Ordering::SeqCst);
        m.shutdown.notify_waiters();
        let drive = m.drive.lock().unwrap_or_else(|error| error.into_inner()).clone();
        let sc = m.shortcut.lock().unwrap_or_else(|error| error.into_inner()).clone();
        let _ = tokio::task::spawn_blocking(move || {
            unmount_system(&drive);
            drop_shortcut(&sc); // 断开就把桌面图标收走,不留一个点开报错的死图标
        })
        .await;
    }
    Ok(())
}


//! Same-account enrollment preserves existing explicit permissions.
use super::*;

pub fn auto_grant_self(
    node_id: &str,
    uid: &str,
    name: &str,
    _is_first_device: bool,
) -> Result<(Grant, bool), String> {
    crate::collab::device_label::validate_characters(name)?;
    let node_id = node_id.trim();
    if node_id.is_empty() {
        return Err("缺 NodeId".into());
    }
    if let Some(g) = get(node_id)? {
        return Ok((g, false));
    }
    let t = now();
    let conn = open_db()?;
    conn.execute(
        "INSERT INTO peer_grants(node_id,uid,name,role,fs_access,exec_access,auto_mount,\
         drive_hint,cooldown_until,granted_at,granted_by,revoked) \
         VALUES(?1,?2,?3,'owner',?4,?5,1,'',?6,?7,'auto:same-account',0)",
        params![
            node_id,
            uid.trim(),
            name.trim(),
            FS_RW,
            EXEC_ALLOW,
            0,
            t
        ],
    )
    .map_err(|e| format!("登记设备契约失败: {e}"))?;
    db::audit(
        uid,
        "peer.grant.auto",
        node_id,
        "同账号设备·策略 B 全权",
    );
    Ok((get(node_id)?.ok_or("刚写入的契约读不回来")?, true))
}


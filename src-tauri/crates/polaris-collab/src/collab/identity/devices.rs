//! Identity devices operations; public paths are re-exported by identity.
use super::*;

#[derive(serde::Serialize)]
pub struct Device {
    pub id: String,
    pub user_id: i64,
    pub name: String,
    pub node_id: String,
    pub revoked: bool,
}

/// 登记一台设备到白名单。返回设备 id。
pub fn add_device(user_id: i64, name: &str, node_id: &str) -> Result<String, String> {
    crate::collab::device_label::validate_characters(name)?;
    let conn = open_db()?;
    let id = random_device_id()?;
    conn.execute(
        "INSERT INTO devices(id,user_id,name,node_id,added_at) VALUES(?1,?2,?3,?4,?5)",
        params![id, user_id, name, node_id, now()],
    )
    .map_err(|e| format!("登记设备失败: {e}"))?;
    db::audit("owner", "device.add", node_id, name);
    Ok(id)
}

/// 遥测/接入:确保 (user_id, node_id) 有一台设备行,并防顶替。
///  - node_id 已属该用户 → 返回 (id, false=非新建)
///  - node_id 属**别的**用户 → Err(拒绝,阻止成员用别台的 node_id 上报假数据)
///  - node_id 空闲 → 自动登记一台(owner 手机 login/PLRK1 只写 sessions 不写 devices,
///    这里补上设备行,遥测才有卡片承载),返回 (id, true=新建)
pub fn ensure_device_for(
    user_id: i64,
    node_id: &str,
    name: &str,
) -> Result<(String, bool), String> {
    crate::collab::device_label::validate_characters(name)?;
    if node_id.trim().is_empty() {
        return Err("node_id 为空".into());
    }
    let conn = open_db()?;
    match conn.query_row(
        "SELECT id,user_id FROM devices WHERE node_id=?1",
        params![node_id],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
    ) {
        Ok((id, owner)) => {
            if owner != user_id {
                return Err("该设备已属其他账号,拒绝顶替".into());
            }
            Ok((id, false))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            let id = random_device_id()?;
            conn.execute(
                "INSERT INTO devices(id,user_id,name,node_id,added_at) VALUES(?1,?2,?3,?4,?5)",
                params![id, user_id, name, node_id, now()],
            )
            .map_err(|e| format!("登记设备失败: {e}"))?;
            Ok((id, true))
        }
        Err(e) => Err(e.to_string()),
    }
}

/// 吊销设备并删除它的全部 HTTP/WS 会话。已建立的长连接会在断开/重连时失效；
/// 真正“即时踢线”还需要连接注册表主动 close，不能在 UI 中宣称已经完成。
/// 返回该设备的 node_id —— 调用方据此同步踢掉云机中继上的挂牌(gateway::unregister_node)。
pub fn revoke_device(device_id: &str) -> Result<String, String> {
    let mut conn = open_db()?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| format!("吊销事务失败: {e}"))?;
    let node_id: String = tx
        .query_row(
            "SELECT node_id FROM devices WHERE id=?1",
            params![device_id],
            |r| r.get(0),
        )
        .map_err(|_| "设备不存在".to_string())?;
    tx.execute(
        "UPDATE devices SET revoked=1 WHERE id=?1",
        params![device_id],
    )
    .map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM sessions WHERE device_id=?1", params![node_id])
        .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| format!("提交吊销失败: {e}"))?;
    // 会话短缓存立即失效:否则被吊销设备的 token 还能在缓存 TTL(10s)内继续鉴权通过。
    auth::bump_session_revocation();
    db::audit("owner", "device.revoke", device_id, &node_id);
    Ok(node_id)
}

/// 某 NodeId 是否在白名单内且未吊销 —— tunnel.rs 准入判定的确定性入口。
pub fn is_node_allowed(node_id: &str) -> bool {
    match open_db() {
        Ok(conn) => conn
            .query_row(
                "SELECT COUNT(*) FROM devices WHERE node_id=?1 AND revoked=0",
                params![node_id],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n > 0)
            .unwrap_or(false),
        Err(_) => false,
    }
}

/// 列白名单设备（owner 管理面）。
pub fn list_devices() -> Result<Vec<Device>, String> {
    let conn = open_db()?;
    let mut stmt = conn
        .prepare("SELECT id,user_id,name,node_id,revoked FROM devices ORDER BY added_at DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Device {
                id: r.get(0)?,
                user_id: r.get(1)?,
                name: r.get(2)?,
                node_id: r.get(3)?,
                revoked: r.get::<_, i64>(4)? != 0,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}


//! Durable per-device enrollment inbox, delivered by heartbeat and acknowledged explicitly.
use crate::collab::db;
use rusqlite::{params, Connection};
use serde_json::{json, Value};

pub static CHANGED: tokio::sync::Notify = tokio::sync::Notify::const_new();

pub fn wake() { CHANGED.notify_waiters(); }

pub(crate) fn enrolled(conn: &Connection, uid: &str, node: &str) -> Result<(), String> {
    let (device, name, platform, time): (String, String, String, i64) = conn.query_row(
        "SELECT d.device_id,m.name,m.os,d.enrolled_at FROM mesh_device_ids d
         JOIN mesh_nodes m ON m.node_id=d.node_id WHERE d.uid=?1 AND d.node_id=?2",
        params![uid, node], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    ).map_err(|e| e.to_string())?;
    let payload = json!({"kind":"device_enrolled", "deviceId":device, "nodeId":node,
        "name":name, "platform":platform, "time":time, "action":"revoke_device"});
    conn.execute(
        "INSERT OR IGNORE INTO mesh_notifications(recipient,subject,payload)
         SELECT d.device_id,?2,?3 FROM mesh_device_ids d JOIN mesh_nodes m ON m.node_id=d.node_id
         WHERE d.uid=?1 AND m.revoked=0 AND d.device_id<>?2
         AND d.sequence<(SELECT sequence FROM mesh_device_ids WHERE device_id=?2)",
        params![uid, device, payload.to_string()],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn inbox(uid: &str, node: &str) -> Result<Vec<Value>, String> {
    let device = super::devices::active(uid, node)?;
    db::with_conn(|conn| {
        let mut st = conn.prepare("SELECT payload FROM mesh_notifications WHERE recipient=?1 AND acknowledged=0")
            .map_err(|e| e.to_string())?;
        let rows = st.query_map(params![device.device_id], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        rows.map(|raw| serde_json::from_str(&raw.map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())).collect()
    })
}

pub fn acknowledge(uid: &str, node: &str, subject: &str) -> Result<(), String> {
    let device = super::devices::active(uid, node)?;
    db::open_db()?.execute("UPDATE mesh_notifications SET acknowledged=1 WHERE recipient=?1 AND subject=?2",
        params![device.device_id, subject]).map_err(|e| e.to_string())?;
    wake();
    Ok(())
}

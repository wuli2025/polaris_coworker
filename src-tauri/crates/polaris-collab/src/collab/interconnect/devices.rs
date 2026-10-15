//! Authority directory generations. Re-enrolling a revoked NodeId creates a new device ID.
use crate::collab::{db, mesh};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Device {
    pub device_id: String,
    pub node_id: String,
    pub enrolled_at: i64,
    pub sequence: i64,
}

pub(crate) fn migrate(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(include_str!("security.sql")).map_err(|e| e.to_string())
}

pub fn active(uid: &str, node: &str) -> Result<Device, String> {
    db::with_conn(|conn| {
        conn.query_row(
            "SELECT d.device_id,d.node_id,d.enrolled_at,d.sequence FROM mesh_device_ids d
             JOIN mesh_nodes m ON m.node_id=d.node_id AND m.uid=d.uid
             WHERE d.uid=?1 AND d.node_id=?2 AND m.revoked=0",
            params![uid, node], |r| Ok(Device {
                device_id: r.get(0)?, node_id: r.get(1)?, enrolled_at: r.get(2)?, sequence: r.get(3)?,
            }),
        ).map_err(|e| format!("设备未登记或已吊销: {e}"))
    })
}

pub fn active_devices(uid: &str) -> Result<Vec<Device>, String> {
    mesh::devices(uid)?.into_iter().filter(|(_, revoked)| !revoked)
        .map(|(peer, _)| active(uid, &peer.node_id)).collect()
}

/// Called inside the directory enrollment transaction, after updating mesh_nodes.
pub(crate) fn enrolled(conn: &Connection, uid: &str, node: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO mesh_device_ids(node_id,uid,device_id,enrolled_at,sequence)
         SELECT node_id,uid,'dev_' || lower(hex(randomblob(16))),created_at,
         (SELECT COALESCE(MAX(sequence),0)+1 FROM mesh_device_ids) FROM mesh_nodes WHERE node_id=?1
         ON CONFLICT(node_id) DO UPDATE SET uid=excluded.uid,device_id=excluded.device_id,
         enrolled_at=excluded.enrolled_at,sequence=excluded.sequence
         WHERE mesh_device_ids.uid<>excluded.uid OR EXISTS(
             SELECT 1 FROM mesh_revocations r WHERE r.uid=mesh_device_ids.uid
             AND r.device_id=mesh_device_ids.device_id)", params![node],
    ).map_err(|e| e.to_string())?;
    // UID is checked here as well as in the caller to keep this boundary explicit.
    let matches: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM mesh_device_ids WHERE node_id=?1 AND uid=?2)",
        params![node, uid], |r| r.get(0),
    ).map_err(|e| e.to_string())?;
    if !matches { return Err("设备账号不匹配".into()); }
    conn.execute("INSERT OR IGNORE INTO mesh_first_device(uid,device_id)
        SELECT uid,device_id FROM mesh_device_ids WHERE uid=?1 AND node_id=?2",
        params![uid, node]).map_err(|e| e.to_string())?;
    super::notifications::enrolled(conn, uid, node)?;
    Ok(())
}

pub(crate) fn revoked(conn: &Connection, uid: &str, node: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO mesh_revocation_versions(uid,version) VALUES(?1,1)
         ON CONFLICT(uid) DO UPDATE SET version=version+1", params![uid],
    ).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR IGNORE INTO mesh_revocations(uid,device_id,node_id,revoked_at,version)
         SELECT d.uid,d.device_id,d.node_id,?3,v.version FROM mesh_device_ids d
         JOIN mesh_revocation_versions v ON v.uid=d.uid WHERE d.uid=?1 AND d.node_id=?2",
        params![uid, node, db::now()],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

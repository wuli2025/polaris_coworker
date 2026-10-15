//! Policy B approval state machine. Authorization binds actor, host and exact operation digest.
use crate::collab::{auth, db};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

#[derive(Debug, Serialize, Deserialize)]
pub struct Permit {
    pub uid: String,
    pub actor: String,
    pub host: String,
    pub digest: String,
    pub nonce: String,
    pub expires_at: i64,
}

pub fn digest(operation: &str, payload: &Value) -> String {
    hex::encode(Sha256::digest(format!("{operation}\n{payload}")))
}

fn valid_operation(operation: &str) -> bool {
    super::risk_inventory::HIGH_RISK_OPERATIONS.iter().any(|op| op.id == operation)
}

/// Authority-side gate. The authenticated transport supplies host_node; caller cannot select it.
pub fn authorize(
    uid: &str, actor_node: &str, host_node: &str, operation: &str, digest: &str, nonce: &str,
) -> Result<Permit, String> {
    authorize_on(uid, actor_node, uid, host_node, operation, digest, nonce)
}

pub fn authorize_on(
    uid: &str, actor_node: &str, host_uid: &str, host_node: &str,
    operation: &str, digest: &str, nonce: &str,
) -> Result<Permit, String> {
    if !valid_operation(operation) || digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit())
        || nonce.is_empty() || nonce.len() > 128
    { return Err("高风险操作参数无效".into()); }
    let config = super::load()?.security;
    let mut conn = db::open_db()?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let actor = super::devices::active(uid, actor_node)?;
    let host = super::devices::active(host_uid, host_node)?;
    let devices = super::devices::active_devices(uid)?;
    let first: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM mesh_first_device WHERE uid=?1 AND device_id=?2)",
        params![uid, actor.device_id], |r| r.get(0),
    ).map_err(|e| e.to_string())?;
    let needs_approval = !first && devices.len() > 1
        && db::now() < actor.enrolled_at + (config.new_device_window_hours * 3600) as i64;
    if needs_approval {
        let previous: Option<(String, String, i64, String)> = tx.query_row(
            "SELECT id,state,expires_at,approver FROM mesh_approvals WHERE uid=?1 AND actor=?2
             AND host=?3 AND digest=?4 AND state NOT IN ('consumed','closed') ORDER BY created_at DESC,rowid DESC LIMIT 1",
            params![uid, actor.device_id, host.device_id, digest],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        ).optional().map_err(|e| e.to_string())?;
        match previous {
            Some((id, state, expires, approver)) => {
                if expires <= db::now() || state == "denied" {
                    tx.execute("UPDATE mesh_approvals SET state='closed' WHERE id=?1", params![id])
                        .map_err(|e| e.to_string())?;
                    tx.commit().map_err(|e| e.to_string())?;
                    return Err(format!("approval_expired_or_denied:{id}"));
                }
                if state != "approved" { return Err(format!("approval_pending:{id}")); }
                if !devices.iter().any(|d| d.device_id == approver && d.sequence < actor.sequence) {
                    return Err("批准设备已吊销或不再有效".into());
                }
                tx.execute("UPDATE mesh_approvals SET state='consumed' WHERE id=?1 AND state='approved'",
                    params![id]).map_err(|e| e.to_string())?;
            }
            None => {
                let id = auth::random_token();
                tx.execute(
                    "INSERT INTO mesh_approvals(id,uid,actor,host,digest,operation,created_at,expires_at,state)
                     VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'pending')",
                    params![id, uid, actor.device_id, host.device_id, digest, operation,
                        db::now(), db::now() + config.approval_timeout_secs as i64],
                ).map_err(|e| e.to_string())?;
                tx.commit().map_err(|e| e.to_string())?;
                super::notifications::wake();
                return Err(format!("approval_pending:{id}"));
            }
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(Permit { uid: uid.into(), actor: actor.device_id, host: host.node_id,
        digest: digest.into(), nonce: nonce.into(), expires_at: db::now() + 30 })
}

pub fn decide(uid: &str, node: &str, request: &str, approve: bool) -> Result<(), String> {
    let mut conn = db::open_db()?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let approver = super::devices::active(uid, node)?;
    let changed = tx.execute(
        "UPDATE mesh_approvals SET state=?1,approver=?2 WHERE id=?3 AND uid=?4
         AND state='pending' AND expires_at>?5 AND actor<>?2 AND EXISTS(
         SELECT 1 FROM mesh_device_ids d JOIN mesh_nodes m ON m.node_id=d.node_id
         WHERE d.device_id=mesh_approvals.actor AND d.uid=?4 AND m.revoked=0 AND d.sequence>?6)",
        params![if approve { "approved" } else { "denied" }, approver.device_id, request,
            uid, db::now(), approver.sequence],
    ).map_err(|e| e.to_string())?;
    if changed != 1 { return Err("请求已超时、已处理，或需要接入前已有的设备批准".into()); }
    tx.commit().map_err(|e| e.to_string())?;
    super::notifications::wake();
    Ok(())
}

pub fn pending(uid: &str, node: &str) -> Result<Vec<Value>, String> {
    let device = super::devices::active(uid, node)?;
    db::with_conn(|conn| {
        let mut st = conn.prepare(
            "SELECT a.id,a.operation,a.actor,a.host,a.digest,a.expires_at,a.state FROM mesh_approvals a
             JOIN mesh_device_ids d ON d.device_id=a.actor WHERE a.uid=?1 AND a.expires_at>?2
             AND a.state IN ('pending','approved') AND (d.sequence>?3 OR a.actor=?4)",
        ).map_err(|e| e.to_string())?;
        let rows = st.query_map(params![uid, db::now(), device.sequence, device.device_id], |r| Ok(json!({
            "id":r.get::<_,String>(0)?, "operation":r.get::<_,String>(1)?, "actor":r.get::<_,String>(2)?,
            "host":r.get::<_,String>(3)?, "digest":r.get::<_,String>(4)?,
            "expiresAt":r.get::<_,i64>(5)?, "state":r.get::<_,String>(6)?,
        }))).map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    })
}

//! Device-bound federated sessions. Legacy assertions retain the old admission path with audit.
use crate::collab::{auth, authority::Claims, db, grants};
use rusqlite::{params, OptionalExtension};

pub fn node<'a>(claims: &'a Claims, supplied: &'a str) -> Result<&'a str, String> {
    match (&claims.device_id, &claims.node_id) {
        (Some(device), Some(node)) if !device.is_empty() && !node.is_empty() => {
            if !supplied.trim().is_empty() && supplied.trim() != node && supplied.trim() != device {
                return Err("断言绑定的 NodeId 与请求设备不符".into());
            }
            if super::revocations::is_revoked(&claims.uid, device)? {
                return Err("设备已被权威吊销".into());
            }
            if grants::is_revoked(node) { return Err("设备已被本机撤销".into()); }
            Ok(node)
        }
        (None, None) => {
            eprintln!("[auth] legacy_unbound_assertion uid={}", claims.uid);
            db::audit(&claims.username, "auth.legacy_unbound_assertion", supplied, &claims.uid);
            Ok(supplied.trim())
        }
        _ => Err("断言设备绑定字段不完整".into()),
    }
}

pub fn issue(user_id: i64, claims: &Claims, supplied: &str) -> Result<String, String> {
    let node = node(claims, supplied)?;
    let Some(device) = &claims.device_id else { return auth::issue_session(user_id, node); };
    let token = auth::random_token();
    let mut conn = db::open_db()?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    // Recheck under the same write transaction as insertion; sync cannot race issuance.
    let revoked: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM cached_revocations WHERE uid=?1 AND device_id=?2)
         OR EXISTS(SELECT 1 FROM mesh_revocations WHERE uid=?1 AND device_id=?2)",
        params![claims.uid, device], |r| r.get(0),
    ).map_err(|e| e.to_string())?;
    if revoked { return Err("设备已被权威吊销".into()); }
    auth::insert_session_tx(&tx, &token, user_id, device)?;
    tx.execute("INSERT INTO bound_sessions(token,uid,device_id,node_id) VALUES(?1,?2,?3,?4)",
        params![token, claims.uid, device, node]).map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(token)
}

/// Called before the short session cache on every request. Offline checks stay entirely local.
pub fn check(token: &str) -> Result<(), String> {
    let binding: Option<(String, String)> = db::with_conn(|conn| conn.query_row(
        "SELECT uid,device_id FROM bound_sessions WHERE token=?1", params![token],
        |r| Ok((r.get(0)?, r.get(1)?)),
    ).optional().map_err(|e| e.to_string()))?;
    if let Some((uid, device)) = binding {
        if super::revocations::is_revoked(&uid, &device)? {
            auth::logout(token)?;
            return Err("设备已被权威吊销".into());
        }
    }
    Ok(())
}

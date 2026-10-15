//! Signed, monotonic per-account revocation snapshots; no network on session checks.
use crate::collab::{auth, authority, db};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Revocation {
    pub device_id: String,
    pub node_id: String,
    pub revoked_at: i64,
    pub version: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub uid: String,
    pub version: i64,
    pub issued_at: i64,
    pub revoked: Vec<Revocation>,
    pub devices: Vec<super::devices::Device>,
    pub first_device: Option<String>,
}

pub fn signed(uid: &str) -> Result<String, String> {
    let conn = db::open_db()?;
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    let version = tx.query_row("SELECT version FROM mesh_revocation_versions WHERE uid=?1",
        params![uid], |r| r.get(0)).optional().map_err(|e| e.to_string())?.map_or(0, |v| v);
    let mut st = tx.prepare(
        "SELECT device_id,node_id,revoked_at,version FROM mesh_revocations WHERE uid=?1 ORDER BY version",
    ).map_err(|e| e.to_string())?;
    let revoked = st.query_map(params![uid], |r| Ok(Revocation {
        device_id: r.get(0)?, node_id: r.get(1)?, revoked_at: r.get(2)?, version: r.get(3)?,
    })).map_err(|e| e.to_string())?.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;
    let mut st = tx.prepare("SELECT d.device_id,d.node_id,d.enrolled_at,d.sequence
        FROM mesh_device_ids d JOIN mesh_nodes m ON m.node_id=d.node_id AND m.uid=d.uid
        WHERE d.uid=?1 AND m.revoked=0").map_err(|e| e.to_string())?;
    let devices = st.query_map(params![uid], |r| Ok(super::devices::Device {
        device_id:r.get(0)?, node_id:r.get(1)?, enrolled_at:r.get(2)?, sequence:r.get(3)?,
    })).map_err(|e| e.to_string())?.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;
    let first_device = tx.query_row("SELECT device_id FROM mesh_first_device WHERE uid=?1",
        params![uid], |r| r.get(0)).optional().map_err(|e| e.to_string())?;
    authority::sign_document("PR1", &Snapshot {
        uid: uid.into(), version, issued_at: db::now(), revoked, devices, first_device,
    })
}

pub fn accept(uid: &str, document: &str) -> Result<(), String> {
    let (_, key, _) = authority::pinned().ok_or("吊销清单缺少已钉住的权威公钥")?;
    let config = super::load()?;
    if !config.authority.public_key_fingerprints.is_empty() { config.verify_key(&key)?; }
    accept_with(uid, document, &key)
}

pub(crate) fn accept_with(uid: &str, document: &str, key: &str) -> Result<(), String> {
    let snapshot: Snapshot = authority::verify_document("PR1", document, key)?;
    if snapshot.uid != uid || uid.is_empty() || snapshot.version < 0
        || snapshot.issued_at > db::now() + 30 || snapshot.issued_at <= 0
        || snapshot.revoked.iter().any(|r| r.device_id.is_empty() || r.node_id.is_empty()
            || r.version <= 0 || r.version > snapshot.version || r.revoked_at > snapshot.issued_at)
    { return Err("吊销清单账号、版本或时间无效".into()); }
    let mut conn = db::open_db()?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let previous: Option<(i64, i64)> = tx.query_row(
        "SELECT version,issued_at FROM cached_revocation_lists WHERE uid=?1", params![uid],
        |r| Ok((r.get(0)?, r.get(1)?)),
    ).optional().map_err(|e| e.to_string())?;
    if previous.is_some_and(|(version, issued)| snapshot.version < version || snapshot.issued_at < issued) {
        return Err("吊销清单回退，保留最后有效清单".into());
    }
    for r in &snapshot.revoked {
        let inserted = tx.execute(
            "INSERT OR IGNORE INTO cached_revocations(uid,device_id,node_id,revoked_at) VALUES(?1,?2,?3,?4)",
            params![uid, r.device_id, r.node_id, r.revoked_at],
        ).map_err(|e| e.to_string())?;
        // Only retract existing same-account contracts; authority never creates permissions.
        if inserted > 0 {
            tx.execute("UPDATE peer_grants SET revoked=1 WHERE uid=?1 AND node_id=?2",
                params![uid, r.node_id]).map_err(|e| e.to_string())?;
        }
    }
    tx.execute(
        "DELETE FROM sessions WHERE token IN (SELECT b.token FROM bound_sessions b
         JOIN cached_revocations r ON r.uid=b.uid AND r.device_id=b.device_id)", [],
    ).map_err(|e| e.to_string())?;
    tx.execute(
        "INSERT INTO cached_revocation_lists(uid,version,issued_at,document) VALUES(?1,?2,?3,?4)
         ON CONFLICT(uid) DO UPDATE SET version=excluded.version,issued_at=excluded.issued_at,document=excluded.document",
        params![uid, snapshot.version, snapshot.issued_at, document],
    ).map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    auth::bump_session_revocation();
    Ok(())
}

pub fn is_revoked(uid: &str, device: &str) -> Result<bool, String> {
    db::with_conn(|conn| conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM cached_revocations WHERE uid=?1 AND device_id=?2)
         OR EXISTS(SELECT 1 FROM mesh_revocations WHERE uid=?1 AND device_id=?2)",
        params![uid, device], |r| r.get(0),
    ).map_err(|e| e.to_string()))
}

pub fn require_fresh(uid: &str) -> Result<(), String> {
    let limit = super::load()?.security.revocation_max_age_secs as i64;
    let issued: Option<i64> = db::with_conn(|conn| conn.query_row(
        "SELECT issued_at FROM cached_revocation_lists WHERE uid=?1 AND document<>''",
        params![uid], |r| r.get(0),
    ).optional().map_err(|e| e.to_string()))?;
    if !issued.is_some_and(|t| db::now() - t <= limit) {
        return Err("需要联网确认：吊销清单已过期或尚未同步".into());
    }
    Ok(())
}

/// Poll every federated member account, including guests not owned by the host's mesh account.
pub fn accounts() -> Result<Vec<String>, String> {
    db::with_conn(|conn| {
        let mut st = conn.prepare("SELECT DISTINCT uid FROM users WHERE uid<>''")
            .map_err(|e| e.to_string())?;
        let out = st.query_map([], |r| r.get(0)).map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string());
        out
    })
}

/// Verify cached authority policy data before using it for offline high-risk exemptions.
pub fn cached(uid: &str) -> Result<Snapshot, String> {
    require_fresh(uid)?;
    let (_, key, _) = authority::pinned().ok_or("需要联网确认：尚未信任权威")?;
    let config = super::load()?;
    if !config.authority.public_key_fingerprints.is_empty() { config.verify_key(&key)?; }
    let document: String = db::with_conn(|conn| conn.query_row(
        "SELECT document FROM cached_revocation_lists WHERE uid=?1", params![uid], |r| r.get(0),
    ).map_err(|e| e.to_string()))?;
    let verified = authority::verify_document::<Snapshot>("PR1", &document, &key)
        .and_then(|snapshot| {
            if snapshot.uid != uid { return Err("吊销清单账号不匹配".into()); }
            Ok(snapshot)
        });
    if verified.is_err() {
        // Clear only the damaged payload, retaining the rollback floor and known revocations.
        // Compare the payload so a concurrent successful heartbeat is not discarded.
        db::with_conn(|conn| conn.execute(
            "UPDATE cached_revocation_lists SET document='' WHERE uid=?1 AND document=?2",
            params![uid, document],
        ).map(|_| ()).map_err(|e| e.to_string()))?;
    }
    verified
}

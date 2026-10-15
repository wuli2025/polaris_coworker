CREATE TABLE IF NOT EXISTS mesh_device_ids(
    node_id TEXT PRIMARY KEY, uid TEXT NOT NULL, device_id TEXT NOT NULL UNIQUE,
    enrolled_at INTEGER NOT NULL, sequence INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS mesh_revocation_versions(uid TEXT PRIMARY KEY, version INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS mesh_revocations(
    uid TEXT NOT NULL, device_id TEXT NOT NULL, node_id TEXT NOT NULL,
    revoked_at INTEGER NOT NULL, version INTEGER NOT NULL, PRIMARY KEY(uid,device_id)
);
CREATE TABLE IF NOT EXISTS cached_revocation_lists(
    uid TEXT PRIMARY KEY, version INTEGER NOT NULL, issued_at INTEGER NOT NULL, document TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS cached_revocations(
    uid TEXT NOT NULL, device_id TEXT NOT NULL, node_id TEXT NOT NULL,
    revoked_at INTEGER NOT NULL, PRIMARY KEY(uid,device_id)
);
CREATE TABLE IF NOT EXISTS bound_sessions(
    token TEXT PRIMARY KEY REFERENCES sessions(token) ON DELETE CASCADE,
    uid TEXT NOT NULL, device_id TEXT NOT NULL, node_id TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS bound_session_device ON bound_sessions(uid,device_id);
-- Backfill independent device IDs; the node primary key keeps migration idempotent.
INSERT OR IGNORE INTO mesh_device_ids(node_id,uid,device_id,enrolled_at,sequence)
SELECT node_id,uid,'dev_' || lower(hex(randomblob(16))),first_seen,rowid FROM mesh_nodes;

CREATE TABLE IF NOT EXISTS mesh_notifications(
    recipient TEXT NOT NULL, subject TEXT NOT NULL, payload TEXT NOT NULL,
    acknowledged INTEGER NOT NULL DEFAULT 0, PRIMARY KEY(recipient,subject)
);
CREATE TABLE IF NOT EXISTS mesh_approvals(
    id TEXT PRIMARY KEY, uid TEXT NOT NULL, actor TEXT NOT NULL, host TEXT NOT NULL,
    digest TEXT NOT NULL, operation TEXT NOT NULL, created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL, state TEXT NOT NULL, approver TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS mesh_approval_lookup ON mesh_approvals(uid,actor,host,digest);
CREATE TABLE IF NOT EXISTS mesh_first_device(uid TEXT PRIMARY KEY, device_id TEXT NOT NULL);
INSERT OR IGNORE INTO mesh_first_device(uid,device_id)
SELECT uid,device_id FROM mesh_device_ids GROUP BY uid HAVING sequence=MIN(sequence);
-- Previously revoked directory entries must also invalidate assertions after upgrade.
INSERT OR IGNORE INTO mesh_revocation_versions(uid,version)
SELECT uid,COUNT(*) FROM mesh_nodes WHERE revoked<>0 GROUP BY uid;
INSERT OR IGNORE INTO mesh_revocations(uid,device_id,node_id,revoked_at,version)
SELECT d.uid,d.device_id,d.node_id,CAST(strftime('%s','now') AS INTEGER),v.version
FROM mesh_device_ids d JOIN mesh_nodes m ON m.node_id=d.node_id
JOIN mesh_revocation_versions v ON v.uid=d.uid WHERE m.revoked<>0;
-- Remove the obsolete observation period without changing explicit access/revocation choices.
UPDATE peer_grants SET cooldown_until=0 WHERE cooldown_until<>0;

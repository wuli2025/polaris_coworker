//! collab/identity.rs —— 主机身份、一次性邀请票据、设备白名单。
//!
//! host.key = 主机长期身份密钥（600 权限，永不出主机）。邀请票据 = 一次性配对码，
//! 24h 有效、用后即废。设备白名单 = 隧道层准入键（iroh NodeId），双因子的「设备」那一因子。
use base64::Engine;
use rusqlite::params;
#[cfg(unix)]
use std::io::Write;
use std::path::PathBuf;

use super::auth;
use super::db::{self, now, open_db};

const TICKET_TTL: i64 = 24 * 3600;

/// host.key 落位：`~/Polaris/data/host.key`，可经 `POLARIS_HOST_KEY` 覆写。
fn host_key_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("POLARIS_HOST_KEY") {
        let p = p.trim();
        if !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    directories::UserDirs::new().map(|u| u.home_dir().join("Polaris").join("data").join("host.key"))
}

/// 取（或首次生成）主机身份密钥，返回其 32 字节种子（base64url）。
///
/// 这里用 32 字节随机种子作为主机身份根：iroh 的 SecretKey 也是 32 字节种子，
/// tunnel.rs 上线时直接 `iroh::SecretKey::from_bytes(seed)` 即可，无需二次生成。
pub fn get_or_create_host_key() -> Result<[u8; 32], String> {
    let path = host_key_path().ok_or("无法定位用户目录")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("建数据目录失败: {e}"))?;
    }
    if path.exists() {
        let raw = std::fs::read_to_string(&path).map_err(|e| format!("读 host.key 失败: {e}"))?;
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(raw.trim())
            .map_err(|e| format!("host.key 损坏: {e}"))?;
        if bytes.len() != 32 {
            return Err("host.key 长度异常".into());
        }
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&bytes);
        return Ok(seed);
    }
    let mut seed = [0u8; 32];
    getrandom::getrandom(&mut seed).map_err(|e| format!("生成密钥失败: {e}"))?;
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(seed);
    // 尽量以 600 权限落盘。
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)
            .map_err(|e| format!("写 host.key 失败: {e}"))?;
        f.write_all(encoded.as_bytes()).map_err(|e| e.to_string())?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(&path, encoded.as_bytes()).map_err(|e| format!("写 host.key 失败: {e}"))?;
    }
    db::audit("host", "hostkey.create", "", "");
    Ok(seed)
}

/// host.key 的公开指纹（sha256 前 16 字节 hex）——用于成员端固定信任、云机镜像标识。
pub fn host_fingerprint() -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let seed = get_or_create_host_key()?;
    let digest = Sha256::digest(seed);
    Ok(digest[..8].iter().map(|b| format!("{b:02x}")).collect())
}

// ───────────────────────── 邀请票据 ─────────────────────────

#[derive(serde::Serialize)]
pub struct Ticket {
    pub code: String,
    pub role: String,
    pub expires_at: i64,
}

fn random_code() -> String {
    // 人可读配对码：8 位大写字母数字（去掉易混的 0/O/1/I）。
    const ALPH: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut buf = [0u8; 8];
    getrandom::getrandom(&mut buf).expect("getrandom");
    buf.iter().map(|b| ALPH[(*b as usize) % ALPH.len()] as char).collect()
}

/// 签发一次性邀请票据。role 决定兑换后账号的角色。
pub fn create_ticket(role: &str, note: &str) -> Result<Ticket, String> {
    let conn = open_db()?;
    let code = random_code();
    let t = now();
    conn.execute(
        "INSERT INTO tickets(code,role,created_at,expires_at,note) VALUES(?1,?2,?3,?4,?5)",
        params![code, role, t, t + TICKET_TTL, note],
    )
    .map_err(|e| format!("签发票据失败: {e}"))?;
    db::audit("owner", "ticket.create", role, note);
    Ok(Ticket { code, role: role.into(), expires_at: t + TICKET_TTL })
}

/// 兑换票据 → 建账号 + 登记设备。票据用后即废（幂等：并发下唯一 UPDATE 保证只成一次）。
pub fn redeem_ticket(
    code: &str,
    username: &str,
    password: &str,
    display_name: &str,
    device_name: &str,
    node_id: &str,
) -> Result<(auth::User, String), String> {
    let conn = open_db()?;
    let row = conn.query_row(
        "SELECT role,expires_at,used_at FROM tickets WHERE code=?1",
        params![code.trim()],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, Option<i64>>(2)?)),
    );
    let (role, expires_at, used_at) = row.map_err(|_| "票据无效".to_string())?;
    if used_at.is_some() {
        return Err("票据已被使用".into());
    }
    if expires_at < now() {
        return Err("票据已过期".into());
    }
    // 先原子占用票据，防并发重复兑换。
    let claimed = conn
        .execute(
            "UPDATE tickets SET used_at=?1 WHERE code=?2 AND used_at IS NULL",
            params![now(), code.trim()],
        )
        .map_err(|e| e.to_string())?;
    if claimed != 1 {
        return Err("票据已被使用".into());
    }
    let user = auth::create_user(username, password, &role, display_name)?;
    add_device(user.id, device_name, node_id)?;
    let (_, token) = auth::login(username, password, node_id)?;
    Ok((user, token))
}

// ───────────────────────── 分享码(配对码带地址) ─────────────────────────

/// 分享码:`PLRS1-<base64url(json{c:裸码, a:[地址]})>`。裸码仍单独入库,分享码只是
/// 传输壳——成员端解开后逐个探活地址、自动填主机、再用裸码走 redeem,零手填。
pub fn encode_share_code(code: &str, addrs: &[String]) -> String {
    let payload = serde_json::json!({ "c": code, "a": addrs });
    let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload.to_string());
    format!("PLRS1-{b64}")
}

/// 分享码 → (裸码, 地址表)。不是分享码/结构不符返回 None(裸码走旧流程)。
pub fn decode_share_code(s: &str) -> Option<(String, Vec<String>)> {
    let b64 = s.trim().strip_prefix("PLRS1-")?;
    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(b64).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&raw).ok()?;
    let code = v.get("c")?.as_str()?.to_string();
    let addrs = v
        .get("a")?
        .as_array()?
        .iter()
        .filter_map(|x| x.as_str().map(String::from))
        .collect();
    Some((code, addrs))
}

// ───────────────────────── 设备白名单 ─────────────────────────

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
    let conn = open_db()?;
    let mut id = [0u8; 12];
    getrandom::getrandom(&mut id).map_err(|e| e.to_string())?;
    let id = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(id);
    conn.execute(
        "INSERT INTO devices(id,user_id,name,node_id,added_at) VALUES(?1,?2,?3,?4,?5)",
        params![id, user_id, name, node_id, now()],
    )
    .map_err(|e| format!("登记设备失败: {e}"))?;
    db::audit("owner", "device.add", node_id, name);
    Ok(id)
}

/// 吊销设备（两分钟止血）。隧道层准入即刻拒绝该 NodeId。
pub fn revoke_device(device_id: &str) -> Result<(), String> {
    let conn = open_db()?;
    conn.execute("UPDATE devices SET revoked=1 WHERE id=?1", params![device_id])
        .map_err(|e| e.to_string())?;
    db::audit("owner", "device.revoke", device_id, "");
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_code_roundtrip() {
        let addrs = vec![
            "http://192.168.1.5:8484".to_string(),
            "http://100.1.2.3:8484".to_string(),
        ];
        let s = encode_share_code("ABCD2345", &addrs);
        assert!(s.starts_with("PLRS1-"));
        let (code, back) = decode_share_code(&s).unwrap();
        assert_eq!(code, "ABCD2345");
        assert_eq!(back, addrs);
    }

    #[test]
    fn share_code_rejects_garbage() {
        assert!(decode_share_code("ABCD2345").is_none()); // 裸码不是分享码
        assert!(decode_share_code("PLRS1-!!!not-b64").is_none()); // 坏 base64
        assert!(decode_share_code("PLRS1-e30").is_none()); // 合法 b64 但缺字段({})
    }
}

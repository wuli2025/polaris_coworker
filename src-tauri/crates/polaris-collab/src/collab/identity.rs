//! collab/identity.rs —— 主机身份、一次性邀请票据、设备白名单。
//!
//! host.key = 主机长期身份密钥（600 权限，永不出主机）。邀请票据 = 一次性配对码，
//! 24h 有效、用后即废。设备白名单 = 隧道层准入键（iroh NodeId），双因子的「设备」那一因子。
use base64::Engine;
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use rusqlite::{params, TransactionBehavior};
#[cfg(unix)]
use std::io::Write;
use std::path::PathBuf;

use super::auth;
use super::db::{self, now, open_db};

mod tickets;
mod sharing;
mod devices;
pub use tickets::{create_ticket, redeem_ticket, redeem_ticket_existing, Ticket};
pub use sharing::{decode_share_code, decode_share_code_full, encode_share_code, ShareCode};
pub use devices::{add_device, ensure_device_for, is_node_allowed, list_devices, revoke_device, Device};

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

// ───────────────────────── 设备码连接的 NodeId 持有证明 ─────────────────────────

const CONNECT_PROOF_DOMAIN: &[u8] = b"polaris-collab/connect-pop/v1\0";

/// iroh 1.x 的 EndpointId 展示为 32 字节 Ed25519 公钥 hex。所有设备契约都只存这一种
/// 小写规范形，避免同一把公钥靠大小写变出两行 grant。
pub fn canonical_node_id(raw: &str) -> Result<String, String> {
    let text = raw.trim();
    let bytes = hex::decode(text).map_err(|_| "NodeId 不是有效的 Ed25519 公钥".to_string())?;
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| "NodeId 长度异常，应为 32 字节公钥".to_string())?;
    VerifyingKey::from_bytes(&bytes).map_err(|_| "NodeId 不是有效的 Ed25519 公钥".to_string())?;
    Ok(hex::encode(bytes))
}

fn connect_proof_message(node_id: &str, code: &str, challenge: &str) -> Result<Vec<u8>, String> {
    let node_id = canonical_node_id(node_id)?;
    let code = code.trim().to_ascii_uppercase();
    if code.len() != 8 || !code.is_ascii() {
        return Err("设备码格式无效".into());
    }
    let challenge = challenge.trim();
    let challenge_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(challenge)
        .map_err(|_| "连接挑战格式无效".to_string())?;
    if challenge_bytes.len() != 32 {
        return Err("连接挑战长度异常".into());
    }

    let mut out = Vec::with_capacity(CONNECT_PROOF_DOMAIN.len() + 4 * 3 + 128);
    out.extend_from_slice(CONNECT_PROOF_DOMAIN);
    for field in [challenge.as_bytes(), node_id.as_bytes(), code.as_bytes()] {
        let len = u32::try_from(field.len()).map_err(|_| "连接证明字段过长".to_string())?;
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(field);
    }
    Ok(out)
}

/// 用本机 `host.key` 给一次性连接挑战签名。签名前先确认调用方声称的 NodeId 的确由这把
/// 密钥派生；绝不能拿隧道客户端的 `device.key` 代签，它与设备台账里的身份不是一把钥匙。
pub fn sign_connect_proof(node_id: &str, code: &str, challenge: &str) -> Result<String, String> {
    let node_id = canonical_node_id(node_id)?;
    let signing = SigningKey::from_bytes(&get_or_create_host_key()?);
    if hex::encode(signing.verifying_key().to_bytes()) != node_id {
        return Err("本机 NodeId 与 host.key 不匹配，请重启互联服务后重试".into());
    }
    let message = connect_proof_message(&node_id, code, challenge)?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(signing.sign(&message).to_bytes()))
}

/// 验证请求者确实持有其所报 NodeId 对应的 host.key。只暴露这个用途固定的验签入口，
/// 不提供“拿主机身份签任意字节”的泛用 API。
pub fn verify_connect_proof(
    node_id: &str,
    code: &str,
    challenge: &str,
    signature: &str,
) -> Result<(), String> {
    let node_id = canonical_node_id(node_id)?;
    let public_bytes: [u8; 32] = hex::decode(&node_id)
        .expect("canonical NodeId is hex")
        .try_into()
        .expect("canonical NodeId is 32 bytes");
    let verifying = VerifyingKey::from_bytes(&public_bytes)
        .map_err(|_| "NodeId 不是有效的 Ed25519 公钥".to_string())?;
    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(signature.trim())
        .map_err(|_| "连接签名格式无效".to_string())?;
    let signature_bytes: [u8; 64] = raw.try_into().map_err(|_| "连接签名长度异常".to_string())?;
    let signature = ed25519_dalek::Signature::from_bytes(&signature_bytes);
    let message = connect_proof_message(&node_id, code, challenge)?;
    verifying
        .verify_strict(&message, &signature)
        .map_err(|_| "NodeId 持有证明无效".to_string())
}

// ───────────────────────── 设备码(一码) ─────────────────────────
//
// 取代此前的三串码:PLRK1 连接码(裸奔一把 owner 令牌)、PLRS1 分享码、8 位一次性票据。
// 三者的区别全是**我方**的内部概念(谁是主机、谁被邀请、票据烧没烧),对用户毫无意义 ——
// 他心里只有一件事:「把我的码给你,你就能连我」。所以合成一串,而且两边完全对称:
// 每台机器都有自己的码,都能被连,也都能去连别人。「主机」这个词从此不出现在界面上。
//
// 与老连接码的关键差别:码里带的是**访问码**,不是会话令牌。
//  · 老 PLRK1:码 = owner 令牌本身 → 谁拿到谁就是 owner,而且改不了(令牌换了整串码作废,
//    但没有任何 UI 能换它)。
//  · 新设备码:码只是敲门用的口令,由**被连的那台机器**验;验过之后建的是一行
//    `peer_grants` 契约(默认只读),权限由机主在台账上说了算,随时可降可撤。
//    换一串码 = 旧码当场失效 + 靠旧码进来的会话全废。

/// 本机访问码在 meta 里的键。
const ACCESS_CODE_KEY: &str = "device_access_code";

/// 取本机访问码(没有就现生成一串并落库)。
///
/// 这是「一台一串常驻码」的存储层:它跨重启、跨换网、跨换 IP 都不变,
/// 用户复制一次发给谁都行,直到他自己点「换一串」。
pub fn access_code() -> Result<String, String> {
    if let Some(c) = db::meta_get(ACCESS_CODE_KEY) {
        let c = c.trim().to_string();
        if !c.is_empty() {
            return Ok(c);
        }
    }
    let c = random_code();
    db::meta_set(ACCESS_CODE_KEY, &c)?;
    db::audit("system", "devicecode.create", "", "");
    Ok(c)
}

/// 换一串码。旧码当场失效,并把**靠旧码进来的**设备的会话全部作废 ——
/// 「换一串」在用户心里就是「把之前发出去的码全部收回」,只换字符串而不断线是骗人。
///
/// 只清 `granted_by='code'` 那些:同账号自己的设备走的是邮箱登录那条路,与码无关,
/// 不该被换码波及(否则用户换个码,自己的 NAS 盘就掉了)。
pub fn rotate_access_code(actor: &str) -> Result<String, String> {
    let c = random_code();
    let conn = open_db()?;
    // 靠码进来的设备:会话按 device_id(= 对端 NodeId)挂在 sessions 上。
    conn.execute(
        "DELETE FROM sessions WHERE device_id IN \
         (SELECT node_id FROM peer_grants WHERE granted_by=?1)",
        params![super::grants::BY_CODE],
    )
    .map_err(|e| format!("清理旧会话失败: {e}"))?;
    drop(conn);
    db::meta_set(ACCESS_CODE_KEY, &c)?;
    auth::bump_session_revocation();
    db::audit(
        actor,
        "devicecode.rotate",
        "",
        "旧码失效,靠旧码进来的会话已断",
    );
    Ok(c)
}

/// 验一串码。**常量时间**比较:码只有 8 位,逐字节提前返回会把它降成可爆破的长度探测。
pub fn check_access_code(input: &str) -> Result<bool, String> {
    let want = access_code()?;
    let a = want.as_bytes();
    let b = input.trim().to_ascii_uppercase();
    let b = b.as_bytes();
    if a.len() != b.len() {
        return Ok(false);
    }
    let mut diff = 0u8;
    for i in 0..a.len() {
        diff |= a[i] ^ b[i];
    }
    Ok(diff == 0)
}

// ───────────────────────── 邀请票据 ─────────────────────────

fn random_code() -> String {
    // 人可读配对码：8 位大写字母数字（去掉易混的 0/O/1/I）。
    const ALPH: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut buf = [0u8; 8];
    getrandom::getrandom(&mut buf).expect("getrandom");
    buf.iter()
        .map(|b| ALPH[(*b as usize) % ALPH.len()] as char)
        .collect()
}

fn random_device_id() -> Result<String, String> {
    let mut id = [0u8; 12];
    getrandom::getrandom(&mut id).map_err(|e| e.to_string())?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_tmp_db() -> (std::sync::MutexGuard<'static, ()>, PathBuf) {
        let guard = crate::collab::db::TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let path = std::env::temp_dir().join(format!(
            "collab-identity-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::env::set_var("POLARIS_COLLAB_DB", &path);
        (guard, path)
    }

    #[test]
    fn connect_proof_roundtrip_and_field_binding() {
        let signing = SigningKey::from_bytes(&[7u8; 32]);
        let node_id = hex::encode(signing.verifying_key().to_bytes());
        let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([9u8; 32]);
        let message = connect_proof_message(&node_id, "ABCD2345", &challenge).unwrap();
        let signature = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(signing.sign(&message).to_bytes());

        verify_connect_proof(&node_id, "ABCD2345", &challenge, &signature).unwrap();
        assert!(verify_connect_proof(&node_id, "ABCD2346", &challenge, &signature).is_err());
        let other_challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([10u8; 32]);
        assert!(verify_connect_proof(&node_id, "ABCD2345", &other_challenge, &signature).is_err());
    }

    #[test]
    fn connect_proof_cannot_claim_another_node_id() {
        let claimed = SigningKey::from_bytes(&[1u8; 32]);
        let attacker = SigningKey::from_bytes(&[2u8; 32]);
        let node_id = hex::encode(claimed.verifying_key().to_bytes());
        let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([3u8; 32]);
        let message = connect_proof_message(&node_id, "ABCD2345", &challenge).unwrap();
        let forged = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(attacker.sign(&message).to_bytes());

        assert!(verify_connect_proof(&node_id, "ABCD2345", &challenge, &forged).is_err());
    }

    #[test]
    fn connect_proof_rejects_malformed_inputs_and_canonicalizes_node_id() {
        let signing = SigningKey::from_bytes(&[4u8; 32]);
        let node_id = hex::encode(signing.verifying_key().to_bytes());
        assert_eq!(
            canonical_node_id(&node_id.to_ascii_uppercase()).unwrap(),
            node_id
        );
        assert!(canonical_node_id("not-a-node").is_err());
        assert!(canonical_node_id(&"00".repeat(31)).is_err());

        let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([5u8; 32]);
        assert!(verify_connect_proof(&node_id, "ABCD2345", &challenge, "not-base64!").is_err());
        let short_signature = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([0u8; 63]);
        assert!(verify_connect_proof(&node_id, "ABCD2345", &challenge, &short_signature).is_err());
        let short_challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([0u8; 31]);
        assert!(connect_proof_message(&node_id, "ABCD2345", &short_challenge).is_err());
    }

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

    #[test]
    fn invalid_signup_does_not_consume_ticket() {
        let (_guard, path) = with_tmp_db();
        let ticket = create_ticket("collaborator", "atomic").unwrap();
        assert!(redeem_ticket(
            &ticket.code,
            "alice",
            "short",
            "Alice",
            "Alice laptop",
            "node-alice",
        )
        .is_err());
        let (user, token) = redeem_ticket(
            &ticket.code,
            "alice",
            "correct-horse",
            "Alice",
            "Alice laptop",
            "node-alice",
        )
        .unwrap();
        assert_eq!(user.role, "collaborator");
        assert!(!token.is_empty());
        assert!(redeem_ticket(
            &ticket.code,
            "alice2",
            "correct-horse",
            "Alice 2",
            "another laptop",
            "node-alice-2",
        )
        .is_err());
        std::env::remove_var("POLARIS_COLLAB_DB");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn redeem_existing_binds_device_and_records_account() {
        let (_guard, path) = with_tmp_db();
        let alice =
            crate::collab::auth::create_user("alice", "correct-horse", "owner", "Alice").unwrap();
        let bob = crate::collab::auth::create_user("bob", "correct-horse", "collaborator", "Bob")
            .unwrap();
        // 已登录账号凭码入伙:不建新号,设备绑到 alice,票据核销并记 used_by。
        let t = create_ticket("collaborator", "for-alice-laptop").unwrap();
        let dev = redeem_ticket_existing(&t.code, &alice, "Alice 二号机", "node-alice-2").unwrap();
        assert!(!dev.is_empty());
        let conn = open_db().unwrap();
        let used_by: String = conn
            .query_row(
                "SELECT used_by FROM tickets WHERE code=?1",
                params![t.code],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(used_by, "alice");
        // 票据用后即废:同码再兑失败。
        assert!(redeem_ticket_existing(&t.code, &alice, "又一台", "node-x").is_err());
        // 防顶替:bob 拿新票据也不能绑走 alice 的 node_id。
        let t2 = create_ticket("collaborator", "bob-tries").unwrap();
        assert!(redeem_ticket_existing(&t2.code, &bob, "Bob 机", "node-alice-2").is_err());
        std::env::remove_var("POLARIS_COLLAB_DB");
        let _ = std::fs::remove_file(path);
    }

    /// 设备码:一台一串、常驻不变、大小写不敏感。
    #[test]
    fn device_code_is_stable_and_case_insensitive() {
        let (_guard, _path) = with_tmp_db();
        let a = access_code().unwrap();
        assert_eq!(a.chars().count(), 8);
        assert_eq!(access_code().unwrap(), a, "常驻码不能每次调用都变一串");
        assert!(check_access_code(&a).unwrap());
        assert!(
            check_access_code(&a.to_ascii_lowercase()).unwrap(),
            "手打小写也得认"
        );
        assert!(
            check_access_code(&format!("  {a}  ")).unwrap(),
            "粘贴带空格也得认"
        );
        assert!(!check_access_code("ZZZZZZZZ").unwrap());
        assert!(!check_access_code("").unwrap());
        assert!(!check_access_code(&a[..7]).unwrap(), "长度不对必须拒");
    }

    /// 「换一串」= 旧码当场失效 **且** 靠旧码进来的会话断掉。
    /// 只换字符串而不断线是骗人 —— 用户点它的本意就是「把发出去的码全收回」。
    #[test]
    fn rotate_kills_the_old_code_and_its_sessions() {
        let (_guard, _path) = with_tmp_db();
        let old = access_code().unwrap();
        // 一台靠码进来的设备 + 它的会话。
        super::super::grants::grant_by_code("node-guest", "访客机").unwrap();
        // 机主后来点过「信任这台」也仍然是「靠码进来的」；信任只升权限，不能抹掉来源。
        let trusted = super::super::grants::trust_fully("node-guest", "本机").unwrap();
        assert_eq!(trusted.granted_by, super::super::grants::BY_CODE);
        let guest = auth::peer_guest_user().unwrap();
        let tok = auth::issue_session(guest.id, "node-guest").unwrap();
        assert!(auth::check_session(&tok).is_ok());
        // 同账号自己的设备(走邮箱登录那条路)不该被换码波及。
        super::super::grants::auto_grant_self("node-mine", "acct_1", "我的 NAS", true).unwrap();
        let mine = auth::issue_session(guest.id, "node-mine").unwrap();

        let new = rotate_access_code("本机").unwrap();
        assert_ne!(new, old);
        assert!(!check_access_code(&old).unwrap(), "旧码必须当场失效");
        assert!(check_access_code(&new).unwrap());
        assert!(auth::check_session(&tok).is_err(), "靠旧码进来的会话必须断");
        assert!(
            auth::check_session(&mine).is_ok(),
            "自己的设备走的是另一条路,换码不该把它的盘弄掉线"
        );
    }

    #[test]
    fn tickets_cannot_grant_owner() {
        let (_guard, path) = with_tmp_db();
        assert!(create_ticket("owner", "too much privilege").is_err());
        std::env::remove_var("POLARIS_COLLAB_DB");
        let _ = std::fs::remove_file(path);
    }
}

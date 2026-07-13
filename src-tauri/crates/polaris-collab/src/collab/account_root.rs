//! collab/account_root.rs —— 账号根口令(个人设备联盟的身份锚)。
//!
//! 账号根口令 = 一段高熵人可读口令(`PLRA1-…`),owner 注册/首启时生成一次,落 meta,可显示。
//! 作用有二:
//!   1. 在**每台已登录设备**上展示,便于把新设备拉进你的联盟(前端在明显处呈现);
//!   2. 新设备输入口令 → 确定性派生同一「账号锚 key」→ 向已有设备证明「同账号」→ 自动
//!      加入白名单(同账号无障碍互联,免逐台签发邀请票据)。
//!
//! 安全:口令本身即根凭据,泄露 = 别人的设备可进你的联盟。故展示需谨慎;派生出的账号锚
//! key 只用于本机比对/组网,**永不出设备**。这条与 host.key「永不出主机」同源。
use sha2::{Digest, Sha256};

use super::db;

/// meta 表键:本机绑定的账号根口令(原样存,便于再次展示;比对/派生走规范化)。
const META_KEY: &str = "account_root_code";
/// 派生域分隔:确保账号锚 key 不与其它用途的哈希撞用。
const DERIVE_DOMAIN: &[u8] = b"polaris/account-root/v1";
/// 去易混字符(0/O/1/I)的大写字母数字 —— 与邀请码同款字母表,人可读、可口述。
const ALPH: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

#[derive(serde::Serialize, Clone, Debug)]
pub struct AccountRoot {
    /// 人可读口令(展示 + 新设备输入)。形如 `PLRA1-XXXX-XXXX-XXXX-XXXX`。
    pub code: String,
    /// 账号锚指纹(口令派生 key 的 sha256 前 8 字节 hex)—— 设备间比对「同账号」用,可公开。
    pub anchor: String,
}

fn random_group() -> String {
    let mut buf = [0u8; 4];
    getrandom::getrandom(&mut buf).expect("getrandom");
    buf.iter()
        .map(|b| ALPH[(*b as usize) % ALPH.len()] as char)
        .collect()
}

/// 生成一段新账号根口令(不落库)。4 组 × 4 字符,字母表 31 → ≈ 79 bit 熵,足够抗猜。
fn gen_code() -> String {
    format!(
        "PLRA1-{}-{}-{}-{}",
        random_group(),
        random_group(),
        random_group(),
        random_group()
    )
}

/// 规范化口令:去空白与连字符、统一大写 —— 容忍用户输入的格式差异后再派生/比对。
pub fn normalize(code: &str) -> String {
    code.chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

/// 口令 → 32 字节账号锚 key(确定性派生)。口令本身高熵,单次 sha256 + 域分隔已足够;
/// 32 字节同时可直接当 iroh SecretKey 种子,让「账号锚」在网络层也有一个可发现的 NodeId。
pub fn derive_account_key(code: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(DERIVE_DOMAIN);
    h.update(normalize(code).as_bytes());
    let out = h.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&out);
    key
}

/// 账号锚指纹(可公开):设备间用它比对是否「同账号」,不暴露口令与 key 本体。
pub fn anchor_of(code: &str) -> String {
    let key = derive_account_key(code);
    let d = Sha256::digest(key);
    d[..8].iter().map(|b| format!("{b:02x}")).collect()
}

/// 取(或首次生成并落库)本机账号根口令。owner 首启即有,之后恒定。
pub fn get_or_create() -> Result<AccountRoot, String> {
    if let Some(code) = db::meta_get(META_KEY) {
        if !code.trim().is_empty() {
            return Ok(AccountRoot {
                anchor: anchor_of(&code),
                code,
            });
        }
    }
    let code = gen_code();
    db::meta_set(META_KEY, &code)?;
    db::audit("owner", "account_root.create", &anchor_of(&code), "");
    Ok(AccountRoot {
        anchor: anchor_of(&code),
        code,
    })
}

/// 读取本机账号根口令(不存在则返回 None,不自动生成 —— 供「是否已建联盟」判定)。
pub fn peek() -> Option<AccountRoot> {
    db::meta_get(META_KEY).filter(|c| !c.trim().is_empty()).map(|code| AccountRoot {
        anchor: anchor_of(&code),
        code,
    })
}

/// 用已有口令把本机绑定进一个联盟(新设备加入:输入口令即写入本机 meta)。
/// 幂等:已是同一口令 → 无副作用;本机已绑定**不同**口令 → 拒绝,防止把两个联盟误并。
pub fn bind(code: &str) -> Result<AccountRoot, String> {
    let norm = normalize(code);
    if norm.len() < 8 {
        return Err("账号根口令过短或无效".into());
    }
    if let Some(existing) = db::meta_get(META_KEY) {
        if !existing.trim().is_empty() && normalize(&existing) != norm {
            return Err("本机已绑定另一个账号根;请先在设置中解绑,再加入新的联盟".into());
        }
    }
    db::meta_set(META_KEY, code.trim())?;
    db::audit("owner", "account_root.bind", &anchor_of(code), "");
    Ok(AccountRoot {
        anchor: anchor_of(code),
        code: code.trim().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn with_tmp_db() -> (std::sync::MutexGuard<'static, ()>, PathBuf) {
        let guard = crate::collab::db::TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let path = std::env::temp_dir().join(format!(
            "collab-acctroot-{}-{}.db",
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
    fn derive_is_deterministic_and_format_tolerant() {
        // 同一口令、不同书写格式(空格/小写/多连字符)派生同一账号锚。
        let a = anchor_of("PLRA1-ABCD-2345-EFGH-6789");
        let b = anchor_of("  plra1 abcd2345 efgh6789  ");
        assert_eq!(a, b, "规范化后应派生同一锚");
        // 不同口令 → 不同锚。
        assert_ne!(a, anchor_of("PLRA1-ZZZZ-2345-EFGH-6789"));
        assert_eq!(a.len(), 16, "锚 = 8 字节 hex");
    }

    #[test]
    fn gen_code_shape() {
        let c = gen_code();
        assert!(c.starts_with("PLRA1-"));
        let groups: Vec<&str> = c.trim_start_matches("PLRA1-").split('-').collect();
        assert_eq!(groups.len(), 4);
        assert!(groups.iter().all(|g| g.len() == 4));
    }

    #[test]
    fn create_is_idempotent_and_bind_guards_mismatch() {
        let (_g, path) = with_tmp_db();
        let first = get_or_create().unwrap();
        let again = get_or_create().unwrap();
        assert_eq!(first.code, again.code, "已建则恒定,不重新生成");
        // 同口令 rebind 幂等。
        assert!(bind(&first.code).is_ok());
        // 换个口令绑定应被拒(防误并联盟)。
        assert!(bind("PLRA1-XXXX-XXXX-XXXX-XXXX").is_err());
        std::env::remove_var("POLARIS_COLLAB_DB");
        let _ = std::fs::remove_file(path);
    }
}

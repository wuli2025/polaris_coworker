//! collab/auth.rs —— 账号与会话（应用层密码，隧道层设备白名单构成双因子）。
//!
//! 密码用 argon2id 存 PHC 串（内含盐与参数），永不落明文。会话 token 随机 32 字节 base64url。
use argon2::password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use base64::Engine;
use once_cell::sync::Lazy;
use rusqlite::params;
use std::collections::HashMap;
use std::sync::Mutex;

use super::db::{self, now, open_db};

/// 会话有效期（秒）。默认 30 天。
const SESSION_TTL: i64 = 30 * 24 * 3600;

// ── 在线暴破节流(内存态,按用户名)──────────────────────────────────────────
// 连续登录失败达阈值后进冷却窗口拒绝;冷却随失败升级(30s 起、每次翻倍、封顶 300s)。成功即清零。
// 只做「冷却」不做「永久锁定」——永久锁定会被人拿去 DoS 锁别人账号;冷却已足以把在线暴破打到无意义。
static LOGIN_GATE: Lazy<Mutex<HashMap<String, (u32, i64)>>> = Lazy::new(|| Mutex::new(HashMap::new()));
const LOGIN_FAIL_THRESHOLD: u32 = 5;

fn login_cooldown_check(username: &str) -> Result<(), String> {
    let g = LOGIN_GATE.lock().unwrap();
    if let Some((fails, until)) = g.get(username) {
        if *fails >= LOGIN_FAIL_THRESHOLD {
            let left = *until - now();
            if left > 0 {
                return Err(format!("登录尝试过于频繁,请 {left} 秒后再试"));
            }
        }
    }
    Ok(())
}

fn login_record_fail(username: &str) {
    let mut g = LOGIN_GATE.lock().unwrap();
    // 简单封顶防内存膨胀:表过大时清掉已过冷却的陈旧条目。
    if g.len() > 5000 {
        let t = now();
        g.retain(|_, (f, until)| *f >= LOGIN_FAIL_THRESHOLD && *until > t);
    }
    let e = g.entry(username.to_string()).or_insert((0, 0));
    e.0 = e.0.saturating_add(1);
    if e.0 >= LOGIN_FAIL_THRESHOLD {
        let over = (e.0 - LOGIN_FAIL_THRESHOLD).min(4); // 0..=4
        let secs = (30i64 << over).min(300); // 30,60,120,240,300
        e.1 = now() + secs;
    }
}

fn login_clear(username: &str) {
    LOGIN_GATE.lock().unwrap().remove(username);
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub role: String,
    pub display_name: String,
    pub disabled: bool,
}

fn hash_password(pw: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(pw.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| format!("密码哈希失败: {e}"))
}

fn verify_password(pw: &str, phc: &str) -> bool {
    match PasswordHash::new(phc) {
        Ok(parsed) => Argon2::default()
            .verify_password(pw.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

/// 32 字节 CSPRNG → base64url。会话 token 与 server 壳自动口令共用。
pub fn random_token() -> String {
    let mut buf = [0u8; 32];
    getrandom::getrandom(&mut buf).expect("getrandom");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf)
}

/// 建账号。用户名唯一。owner 通常是第一个账号；其余由票据兑换而来。
pub fn create_user(username: &str, password: &str, role: &str, display_name: &str) -> Result<User, String> {
    let username = username.trim();
    if username.is_empty() || password.len() < 6 {
        return Err("用户名不能为空、密码至少 6 位".into());
    }
    let phc = hash_password(password)?;
    let conn = open_db()?;
    conn.execute(
        "INSERT INTO users(username,pass_hash,role,display_name,created_at) VALUES(?1,?2,?3,?4,?5)",
        params![username, phc, role, display_name, now()],
    )
    .map_err(|e| {
        if e.to_string().contains("UNIQUE") {
            "用户名已存在".to_string()
        } else {
            format!("建账号失败: {e}")
        }
    })?;
    let id = conn.last_insert_rowid();
    db::audit(username, "user.create", role, "");
    Ok(User { id, username: username.into(), role: role.into(), display_name: display_name.into(), disabled: false })
}

/// 是否还没有任何账号（首启引导建 owner 用）。
pub fn is_bootstrap() -> Result<bool, String> {
    let conn = open_db()?;
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    Ok(n == 0)
}

/// 登录：校验密码 → 签发会话 token。device_id 关联到会话（供 /ws 与命令做设备核对）。
pub fn login(username: &str, password: &str, device_id: &str) -> Result<(User, String), String> {
    let uname = username.trim().to_string();
    // 暴破节流:同一账号连续失败达阈值后进冷却窗口,冷却期内直接拒(不查库、不比对哈希)。
    login_cooldown_check(&uname)?;
    let conn = open_db()?;
    let row = conn.query_row(
        "SELECT id,pass_hash,role,display_name,disabled FROM users WHERE username=?1",
        params![uname],
        |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
            ))
        },
    );
    let (id, phc, role, display_name, disabled) = match row {
        Ok(v) => v,
        Err(_) => {
            login_record_fail(&uname);
            return Err("用户名或密码错误".into());
        }
    };
    if disabled != 0 {
        return Err("账号已停用".into());
    }
    if !verify_password(password, &phc) {
        login_record_fail(&uname);
        return Err("用户名或密码错误".into());
    }
    login_clear(&uname); // 登录成功清零失败计数
    let token = random_token();
    let t = now();
    conn.execute(
        "INSERT INTO sessions(token,user_id,device_id,created_at,expires_at) VALUES(?1,?2,?3,?4,?5)",
        params![token, id, device_id, t, t + SESSION_TTL],
    )
    .map_err(|e| format!("签发会话失败: {e}"))?;
    db::audit(username, "auth.login", device_id, "");
    Ok((User { id, username: username.into(), role, display_name, disabled: false }, token))
}

/// 校验会话 token → 返回用户（check_auth 的核心）。过期或吊销即失败。
pub fn check_session(token: &str) -> Result<User, String> {
    let conn = open_db()?;
    let row = conn.query_row(
        "SELECT u.id,u.username,u.role,u.display_name,u.disabled,s.expires_at \
         FROM sessions s JOIN users u ON u.id=s.user_id WHERE s.token=?1",
        params![token],
        |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
            ))
        },
    );
    let (id, username, role, display_name, disabled, expires_at) =
        row.map_err(|_| "会话无效，请重新登录".to_string())?;
    if disabled != 0 {
        return Err("账号已停用".into());
    }
    if expires_at < now() {
        let _ = conn.execute("DELETE FROM sessions WHERE token=?1", params![token]);
        return Err("会话已过期，请重新登录".into());
    }
    Ok(User { id, username, role, display_name, disabled: false })
}

/// 登出：删会话。
pub fn logout(token: &str) -> Result<(), String> {
    let conn = open_db()?;
    conn.execute("DELETE FROM sessions WHERE token=?1", params![token])
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 列所有账号（owner 管理面用）。
pub fn list_users() -> Result<Vec<User>, String> {
    let conn = open_db()?;
    let mut stmt = conn
        .prepare("SELECT id,username,role,display_name,disabled FROM users ORDER BY id")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok(User {
                id: r.get(0)?,
                username: r.get(1)?,
                role: r.get(2)?,
                display_name: r.get(3)?,
                disabled: r.get::<_, i64>(4)? != 0,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

/// 停用/启用账号（owner 一键止血）。停用即删其所有会话。
pub fn set_user_disabled(user_id: i64, disabled: bool) -> Result<(), String> {
    let conn = open_db()?;
    conn.execute("UPDATE users SET disabled=?1 WHERE id=?2", params![disabled as i64, user_id])
        .map_err(|e| e.to_string())?;
    if disabled {
        conn.execute("DELETE FROM sessions WHERE user_id=?1", params![user_id]).ok();
    }
    db::audit("owner", "user.disable", &user_id.to_string(), &disabled.to_string());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_db() -> std::sync::MutexGuard<'static, ()> {
        let g = super::super::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let p = std::env::temp_dir().join(format!(
            "collab-test-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::env::set_var("POLARIS_COLLAB_DB", p);
        g
    }

    #[test]
    fn hash_roundtrip() {
        let h = hash_password("hunter2!").unwrap();
        assert!(verify_password("hunter2!", &h));
        assert!(!verify_password("wrong", &h));
    }

    #[test]
    fn login_flow() {
        let _g = tmp_db();
        assert!(is_bootstrap().unwrap());
        create_user("alice", "s3cret", "owner", "Alice").unwrap();
        assert!(!is_bootstrap().unwrap());
        let (u, tok) = login("alice", "s3cret", "dev1").unwrap();
        assert_eq!(u.role, "owner");
        assert_eq!(check_session(&tok).unwrap().username, "alice");
        assert!(login("alice", "nope", "dev1").is_err());
        logout(&tok).unwrap();
        assert!(check_session(&tok).is_err());
    }
}

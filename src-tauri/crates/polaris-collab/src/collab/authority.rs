//! collab/authority.rs —— 账号权威(云机签发身份断言,成员主机离线验签)。
//!
//! 背景:此前每台主机各自持有一份 `users` 表,同一个人在桌面、NAS、云机上是三个毫无关系的
//! 账号 —— 「一个账号在哪台机器都能登」做不到。本模块把**云机立为账号权威**,其余主机
//! 退化为「认签名的成员」。
//!
//! 三种运行模式(环境变量覆盖用户配置与随包默认;占位默认使用本地模式):
//!  · `POLARIS_ACCOUNT_AUTHORITY=1`      本进程**就是**权威。持签名私钥,`/api/account/*` 对外
//!                                        发号:注册(强制绑邮箱)、登录、改密,签发身份断言。
//!  · `POLARIS_ACCOUNT_AUTHORITY_URL=…`  本进程是**成员主机**。不再自己判密码,只认权威签的断言;
//!                                        首次接触须匹配预置指纹,钉住后**离线也能验**。
//!  · 都不设                              本地权威(老行为):自己的 users 表说了算。
//!
//! 为什么是「签名断言」而不是「每次请求问云端」:成员主机大多在家里/NAS,网络随时会断。
//! 断言由**客户端**从云机取来、递给主机,主机纯本地验签 —— 主机自己不需要连得上云机。
//! 断言 TTL 只有 5 分钟:它不是会话,只是「我是谁」的一次性证明,进门即换成主机本地会话。
//!
//! 账号的全局身份是 `uid`(权威签发的稳定串),**不是**用户名,更不是各主机各自的 `users.id` —
//! 用户名将来可改,uid 不会。成员主机的 `users` 行退化成「该账号**在本机的**成员资格」:
//! 存角色、存本机的项目关系,密码位写死一个永不可验的哨兵串。

use base64::Engine;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use std::path::PathBuf;

use super::auth::User;
use super::db::{self, now, open_db};

/// 断言有效期:只够「取到手 → 递给主机」这一趟,不当会话使。
const ASSERTION_TTL: i64 = 300;
/// 时钟偏移容忍:两台机器的时间不会分毫不差。
const CLOCK_SKEW: i64 = 120;
/// 成员主机侧联系权威的超时(取公钥/代登录)。连不上要快速失败,别把登录页吊死。
const UPSTREAM_TIMEOUT_SECS: u64 = 10;

const B64: base64::engine::general_purpose::GeneralPurpose =
    base64::engine::general_purpose::URL_SAFE_NO_PAD;

/// 联邦账号在本机 `users.pass_hash` 位上的哨兵。**不是**合法 PHC 串,
/// `verify_password` 见到它必定返回 false —— 即「联邦账号在成员主机上永远无法用密码登进来」,
/// 只能走权威签的断言。这是故意的:密码只有权威见得到。
pub const FEDERATED_SENTINEL: &str = "!federated";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// 本进程就是账号权威(云机)。
    Authority,
    /// 本进程是成员主机,账号权威在这个 URL。
    Delegated(String),
    /// 本地权威(老行为)。
    Local,
}

mod trust;
pub use trust::{mode, is_authority, upstream_url, pinned, ensure_pinned, pin_explicit, repin, verify_assertion};

// ── 签名私钥(仅权威机持有)────────────────────────────────────────────────

fn signing_key_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("POLARIS_ACCOUNT_KEY") {
        if !p.trim().is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    directories::UserDirs::new().map(|u| {
        u.home_dir()
            .join("Polaris")
            .join("data")
            .join("account_authority.key")
    })
}

/// 取(或首次生成)账号签名私钥。32 字节种子,base64url 落盘,unix 下 0600。
/// 与 host.key / gateway.key 分开保管:那两把是**设备/传输**身份,这把是**账号**身份,
/// 泄露后果不同(这把泄了等于谁都能冒充任何账号),故独立一把、独立轮换。
fn signing_key() -> Result<SigningKey, String> {
    let path = signing_key_path().ok_or("找不到用户目录,无法存放账号签名密钥")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("建数据目录失败: {e}"))?;
    }
    if let Ok(raw) = std::fs::read_to_string(&path) {
        let seed = B64
            .decode(raw.trim())
            .map_err(|e| format!("账号签名密钥损坏(不是 base64): {e}"))?;
        let seed: [u8; 32] = seed
            .try_into()
            .map_err(|_| "账号签名密钥损坏(长度不是 32 字节)".to_string())?;
        return Ok(SigningKey::from_bytes(&seed));
    }
    let mut seed = [0u8; 32];
    getrandom::getrandom(&mut seed).map_err(|e| format!("生成账号签名密钥失败: {e}"))?;
    std::fs::write(&path, B64.encode(seed)).map_err(|e| format!("写账号签名密钥失败: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(SigningKey::from_bytes(&seed))
}

/// 权威公钥(base64url)。成员主机钉的就是这一串。
pub fn public_key_b64() -> Result<String, String> {
    Ok(B64.encode(signing_key()?.verifying_key().to_bytes()))
}

/// 公钥指纹:sha256(pubkey) 前 8 字节 hex。用来人眼核对「钉的是不是同一把钥匙」。
pub fn kid_of(pub_b64: &str) -> String {
    use sha2::{Digest, Sha256};
    let bytes = B64.decode(pub_b64).unwrap_or_default();
    let d = Sha256::digest(&bytes);
    d[..8].iter().map(|b| format!("{b:02x}")).collect()
}

pub fn kid() -> Result<String, String> {
    Ok(kid_of(&public_key_b64()?))
}

// ── 身份断言 ──────────────────────────────────────────────────────────────

mod assertions;
pub use assertions::{Claims, sign_assertion, sign_device_assertion, verify_assertion_with};
pub(crate) use assertions::{sign_document, verify_document};

// ── 成员主机侧:钉住权威公钥(TOFU)────────────────────────────────────────

/// 成员主机把一个请求**原样转给**它信任的账号中心,回体也原样带回来。
///
/// 为什么需要它:手机常常只连得上家里那台桌面/NAS(同一 WiFi、没走公网),连不上云机。
/// 没有这条转发,它就发不出验证码、也换不到断言 —— 「用邮箱登录」在那种网络下直接不可用,
/// 只能退回粘连接码。有了它,手机把请求打给身边这台主机,主机替它跑一趟公网。
///
/// 安全上这是**纯转发**:主机不看内容、不改内容、不留内容。凭据(验证码)始终在
/// 客户端与账号中心之间,主机只是根管子 —— 和它替客户端登录(`upstream_login`)同一口径。
pub fn upstream_forward(path: &str, body: &serde_json::Value) -> Result<serde_json::Value, String> {
    // 地址优先取**已钉住的**那个(meta),而不是 `ensure_pinned` 依赖的环境变量 ——
    // 桌面是双击启动的,`POLARIS_ACCOUNT_AUTHORITY_URL` 在那儿根本不存在;它信任的
    // 账号中心是用户登录时由 `pin_explicit` 写进 meta 的。只看环境变量的话,
    // 这条转发在**桌面主机上永远不可用**,而桌面正是手机最常连的那一台。
    let url = match pinned() {
        Some((u, _, _)) => pin_explicit(&u)?.0,
        None => ensure_pinned()
            .map_err(|e| {
                format!(
                    "这台主机还没登录过任何账号中心,替你转发不了({e})—— 请先在它上面用邮箱登录一次"
                )
            })?
            .0,
    };
    let ep = format!("{url}{path}");
    match super::interconnect::http_client::agent(ureq::AgentBuilder::new())?.post(&ep)
        .timeout(std::time::Duration::from_secs(UPSTREAM_TIMEOUT_SECS))
        .send_json(body.clone())
    {
        Ok(r) => r
            .into_json()
            .map_err(|e| format!("账号中心返回的不是 JSON: {e}")),
        // 业务错误原样透传(「验证码不对」「发送太频繁」这些必须让用户看见原文,
        // 糊成「网络错误」会让人对着正确的码反复试)。
        Err(ureq::Error::Status(_, r)) => {
            let v: serde_json::Value = r.into_json().unwrap_or_default();
            Err(v
                .get("error")
                .and_then(|x| x.as_str())
                .unwrap_or("账号中心拒绝了这次请求")
                .to_string())
        }
        Err(e) => Err(format!("这台主机也连不上账号中心: {}", super::interconnect::http_client::diagnostic(&e))),
    }
}

/// 成员主机代客户端向权威登录(客户端只连得上主机、连不上云机时的兜底路径)。
/// 返回权威签的断言。
pub fn upstream_login(username: &str, password: &str) -> Result<String, String> {
    let (url, _) = ensure_pinned()?;
    let ep = format!("{url}/api/account/login");
    let resp = super::interconnect::http_client::agent(ureq::AgentBuilder::new())?.post(&ep)
        .timeout(std::time::Duration::from_secs(UPSTREAM_TIMEOUT_SECS))
        .send_json(serde_json::json!({"username": username, "password": password}));
    match resp {
        Ok(r) => {
            let v: serde_json::Value = r
                .into_json()
                .map_err(|e| format!("账号权威返回的不是 JSON: {e}"))?;
            super::interconnect::apply_received_relays(&v)?;
            v.get("assertion")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string())
                .ok_or_else(|| "账号权威没返回身份断言".into())
        }
        // 权威给的是业务错误(密码错/账号停用)——原样透传,别糊成「网络错误」误导用户。
        Err(ureq::Error::Status(_, r)) => {
            let v: serde_json::Value = r.into_json().unwrap_or_default();
            Err(v
                .get("error")
                .and_then(|x| x.as_str())
                .unwrap_or("登录被账号权威拒绝")
                .to_string())
        }
        Err(e) => Err(format!("连不上账号权威: {}", super::interconnect::http_client::diagnostic(&e))),
    }
}

// ── 成员主机侧:断言 → 本机成员资格 ────────────────────────────────────────

/// 生成全局账号 id。`acct_` + 16 字节随机 base64url。
pub fn new_uid() -> String {
    let mut buf = [0u8; 16];
    getrandom::getrandom(&mut buf).expect("getrandom");
    format!("acct_{}", B64.encode(buf))
}

mod membership;
pub use membership::{Admission, upsert_member, is_member, owner_uid};

/// 断言 → 本机会话(成员主机的登录出口)。验签 → 认成员资格 → 签本机会话 token。
///
/// 准入分三档,按「这张断言是谁的」决定:
///  · uid == 本机主人 → [`Admission::SelfOwned`],直接进(**这就是「不再登两次」**)。
///  · 否则 → [`Admission::ExistingOnly`],已是成员才放行,陌生人走 `join_with_ticket`。
///  · 设备被本机撤销过(peer_grants.revoked)→ 一律拒,连主人本人的设备也拒 ——
///    「丢了电脑」的应急撤销必须比「我是主人」优先,否则捡到电脑的人照样能进。
mod sessions;
pub use sessions::{login_with_assertion, login_with_assertion_local, join_with_ticket};

/// 用户手里那串东西 → 库里的裸码。**邀请码只有一种**,但用户可能粘到四种形态:
/// 管理面直接给的 8 位裸码、带主机地址的分享码 `PLRS1-…`、互联页设备码
/// `PLR1-<裸码>-<NodeId>`、或者手打时按成了小写。都在这里归一，免得
/// 「明明复制了却说码不对」。
pub fn normalize_invite(raw: &str) -> String {
    let s = raw.trim();
    if s.is_empty() {
        return String::new();
    }
    if let Some((code, _)) = super::identity::decode_share_code(s) {
        return code.trim().to_ascii_uppercase();
    }
    let lower = s.to_ascii_lowercase();
    if let Some(body) = lower.strip_prefix("plr1-") {
        if let Some((code, node_id)) = body.split_once('-') {
            if !code.trim().is_empty() && !node_id.trim().is_empty() {
                return code.trim().to_ascii_uppercase();
            }
        }
    }
    s.to_ascii_uppercase()
}

/// 原子核销一张邀请票据,返回它授予的角色。与 identity::redeem_ticket 同口径:
/// 只认 collaborator/visitor(旧的 member 归一为 collaborator),owner 票据不存在。
fn burn_ticket(code: &str, used_by: &str) -> Result<String, String> {
    let mut conn = open_db()?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| format!("兑换事务失败: {e}"))?;
    let (mut role, expires_at, used_at) = tx
        .query_row(
            "SELECT role,expires_at,used_at FROM tickets WHERE code=?1",
            params![code],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, Option<i64>>(2)?,
                ))
            },
        )
        .map_err(|_| "票据无效".to_string())?;
    if role == "member" {
        role = "collaborator".into(); // 兼容旧版 UI 签出的票据
    }
    if !matches!(role.as_str(), "collaborator" | "visitor") {
        return Err("该票据角色已停用,请管理员重新签发最小权限票据".into());
    }
    if used_at.is_some() {
        return Err("票据已被使用".into());
    }
    if expires_at < now() {
        return Err("票据已过期".into());
    }
    let claimed = tx
        .execute(
            "UPDATE tickets SET used_at=?1, used_by=?3 WHERE code=?2 AND used_at IS NULL",
            params![now(), code, used_by],
        )
        .map_err(|e| e.to_string())?;
    if claimed != 1 {
        return Err("票据已被使用".into());
    }
    tx.commit().map_err(|e| format!("提交兑换失败: {e}"))?;
    Ok(role)
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod host_tests;

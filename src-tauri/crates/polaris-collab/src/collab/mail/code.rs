use super::*;
use super::delivery::{Attempt, Stage};
use std::time::Duration;

pub(super) fn deliver_code(
    email: &str, purpose: &str,
    sender: impl FnOnce(&str, &str, &str) -> Result<(), String>,
) -> Result<(), String> {
    let timeout = crate::collab::interconnect::load()?.mail_delivery.timeout_secs;
    deliver_tracked(email, purpose, &Attempt::new(Duration::from_secs(timeout)),
        |to, subject, body, _| sender(to, subject, body))
        .map_err(|stage| format!("邮件投递失败: {}", stage.name()))
}

pub(super) fn deliver_tracked(
    email: &str, purpose: &str, attempt: &Attempt,
    sender: impl FnOnce(&str, &str, &str, Duration) -> Result<(), String>,
) -> Result<(), Stage> {
    let (code, phc) = attempt.run(Stage::Hash, || {
        let code = gen_code()?;
        let phc = auth::hash_password(&code)?;
        Ok((code, phc))
    })?;
    let action = match purpose { "signup" => "注册账号", "login" => "登录", _ => "找回密码" };
    let body = format!(
        "你正在 Polaris 设备联盟{action},验证码:\n\n    {code}\n\n10 分钟内有效。如果这不是你本人的操作,请忽略这封邮件。"
    );
    let remaining = attempt.remaining()?;
    attempt.run(Stage::Smtp, || sender(email, &format!("【Polaris】{action}验证码 {code}"), &body, remaining))?;
    // No late store after a timed-out SMTP call. Bound SQLite lock waiting by the remaining budget.
    attempt.run(Stage::Store, || {
        let mut conn = open_db()?;
        let remaining = attempt.remaining().map_err(|_| "delivery expired")?;
        conn.busy_timeout(remaining).map_err(|e| e.to_string())?;
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        attempt.remaining().map_err(|_| "delivery expired")?;
        let t = now();
        tx.execute(
            "INSERT INTO email_codes(email,purpose,code_hash,attempts,created_at,expires_at)
             VALUES(?1,?2,?3,0,?4,?5)
             ON CONFLICT(email,purpose) DO UPDATE SET code_hash=excluded.code_hash,
             attempts=0, created_at=excluded.created_at, expires_at=excluded.expires_at",
            params![email, purpose, phc, t, t + CODE_TTL],
        ).map_err(|e| e.to_string())?;
        attempt.remaining().map_err(|_| "delivery expired")?;
        tx.commit().map_err(|e| e.to_string())
    })
}

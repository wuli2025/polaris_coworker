//! Serialize attempt accounting and one-time consumption across SQLite connections/processes.
use super::*;
use rusqlite::{OptionalExtension, TransactionBehavior};

pub fn verify_code(email: &str, purpose: &str, code: &str) -> Result<(), String> {
    let email = auth::validate_email(email)?;
    let code = code.trim();
    if code.is_empty() { return Err(INVALID_CODE.into()); }
    let mut conn = open_db()?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let changed = tx.execute(
        "UPDATE email_codes SET attempts=attempts+1
         WHERE email=?1 AND purpose=?2 AND attempts<?3 AND expires_at>=?4",
        params![email, purpose, MAX_ATTEMPTS, now()],
    ).map_err(|e| e.to_string())?;
    if changed != 1 {
        tx.commit().map_err(|e| e.to_string())?;
        // Release the writer before equalizing missing/expired/exhausted responses.
        auth::hash_password(code)?;
        return Err(INVALID_CODE.into());
    }
    let phc: Option<String> = tx.query_row(
        "SELECT code_hash FROM email_codes WHERE email=?1 AND purpose=?2",
        params![email, purpose], |row| row.get(0),
    ).optional().map_err(|e| e.to_string())?;
    let phc = phc.ok_or("验证码事务记录丢失")?;
    if !auth::verify_password(code, &phc) {
        tx.commit().map_err(|e| e.to_string())?;
        return Err(INVALID_CODE.into());
    }
    let consumed = tx.execute(
        "DELETE FROM email_codes WHERE email=?1 AND purpose=?2 AND code_hash=?3
         AND attempts<=?4 AND expires_at>=?5",
        params![email, purpose, phc, MAX_ATTEMPTS, now()],
    ).map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    if consumed != 1 { return Err(INVALID_CODE.into()); }
    if purpose == "signup" && auth::find_user_by_email(&email)?.is_some() {
        return Err(INVALID_CODE.into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "verify_tests.rs"]
mod tests;

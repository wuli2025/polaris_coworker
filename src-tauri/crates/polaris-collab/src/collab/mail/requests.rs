//! Anonymous email requests: account-independent admission, private background delivery.
use super::{auth, configured, db, deliver_code, rate_check, signup_open};

enum Purpose {
    Signup,
    Reset,
    Login,
}

/// Prepared requests have already consumed the same quota regardless of account state.
pub struct CodeRequest {
    email: String,
    purpose: Purpose,
}

impl CodeRequest {
    pub fn prepare(email: &str, purpose: &str, ip: &str) -> Result<Self, String> {
        let purpose = match purpose {
            "signup" => {
                let open = if crate::collab::authority::is_authority() {
                    configured()
                } else {
                    signup_open()
                };
                if !open {
                    return Err("本主机未开放邮箱注册,请联系管理员或用邀请票据入伙".into());
                }
                Purpose::Signup
            }
            "reset" | "login" => {
                if !configured() {
                    return Err("邮箱服务未配置,请联系管理员".into());
                }
                if purpose == "reset" { Purpose::Reset } else { Purpose::Login }
            }
            _ => return Err("purpose 须为 signup / reset / login".into()),
        };
        let email = auth::validate_email(email)?;
        rate_check(&email, ip)?;
        Ok(Self { email, purpose })
    }

    /// Must run off the HTTP response path; sender is explicit for offline behavior tests.
    pub fn deliver(self, sender: impl FnOnce(&str, &str, &str) -> Result<(), String>) -> Result<(), String> {
        let exists = auth::find_user_by_email(&self.email)?.is_some();
        match (self.purpose, exists) {
            (Purpose::Signup, true) => {
                // A previously issued signup code must not survive a later account binding.
                db::open_db()?.execute(
                    "DELETE FROM email_codes WHERE email=?1 AND purpose='signup'",
                    [&self.email],
                ).map_err(|e| e.to_string())?;
                sender(&self.email, "【Polaris】账号提醒", include_str!("existing_account.txt"))
            }
            (Purpose::Signup, false) => deliver_code(&self.email, "signup", sender),
            (Purpose::Reset, true) => deliver_code(&self.email, "reset", sender),
            (Purpose::Login, true) => deliver_code(&self.email, "login", sender),
            (Purpose::Reset | Purpose::Login, false) => Ok(()),
        }
    }
}

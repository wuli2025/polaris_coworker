use super::*;
use crate::collab::{auth, db, interconnect::test_env::TestEnv, mail};
use parking_lot::Mutex;

type Letters = Arc<Mutex<Vec<(String, String, String)>>>;

struct Fixture {
    previous: Vec<(&'static str, Option<std::ffi::OsString>)>,
    _env: TestEnv,
}

impl Fixture {
    fn new() -> Result<Self, String> {
        let env = TestEnv::new("email-privacy")?;
        let mut fixture = Self { previous: vec![], _env: env };
        for key in ["POLARIS_SMTP_USER", "POLARIS_SMTP_PASS", "POLARIS_EMAIL_SIGNUP",
            "POLARIS_ACCOUNT_OPEN_SIGNUP"] {
            fixture.previous.push((key, std::env::var_os(key)));
            std::env::remove_var(key);
        }
        mail::save_config("smtp.example.com", 465, "sender@example.com", "fake", "sender@example.com", true)?;
        db::open_db()?.execute(
            "INSERT INTO users(username,pass_hash,role,display_name,created_at,email)
             VALUES('alice','!test','collaborator','Alice',1,'known@example.com')", [],
        ).map_err(|e| e.to_string())?;
        Ok(fixture)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        for (key, value) in &self.previous {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
}

async fn wire(response: Response) -> Result<(StatusCode, Vec<u8>), String> {
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await
        .map_err(|e| e.to_string())?;
    Ok((status, bytes.to_vec()))
}

async fn send(email: &str, purpose: &str, letters: Letters) -> Result<(StatusCode, Vec<u8>), String> {
    let (response, task) = send_code_with("unknown-peer".into(), json!({"email":email,"purpose":purpose}),
        mail::delivery::Pool::configured()?,
        move |to, subject, body, _| {
            letters.lock().push((to.into(), subject.into(), body.into()));
            Ok(())
        }).await;
    let response = wire(response).await?;
    if let Some(task) = task { task.await.map_err(|e| e.to_string())?; }
    Ok(response)
}

fn clear_rates() -> Result<(), String> {
    db::open_db()?.execute("DELETE FROM login_attempts", []).map_err(|e| e.to_string())?;
    Ok(())
}

fn plant(email: &str, purpose: &str, phc: &str, ttl: i64, attempts: i64) -> Result<(), String> {
    db::open_db()?.execute(
        "INSERT OR REPLACE INTO email_codes(email,purpose,code_hash,attempts,created_at,expires_at)
         VALUES(?1,?2,?3,?4,?5,?6)",
        rusqlite::params![email, purpose, phc, attempts, db::now(), db::now() + ttl],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[tokio::test]
async fn all_purposes_hide_account_state_and_deliver_only_the_expected_mail() -> Result<(), String> {
    let _fixture = Fixture::new()?;
    for purpose in ["signup", "reset", "login"] {
        clear_rates()?;
        if purpose == "signup" {
            plant("known@example.com", purpose, &auth::hash_password("123456")?, 600, 0)?;
        }
        let letters = Letters::default();
        let known = send(" KNOWN@Example.com ", purpose, letters.clone()).await?;
        let unknown = send("new@example.com", purpose, letters.clone()).await?;
        assert_eq!(known, unknown, "{purpose}: status and bytes must match");
        assert_eq!(known, (StatusCode::OK, br#"{"ok":true}"#.to_vec()));
        let delivered = letters.lock().clone();
        assert_eq!(delivered.len(), if purpose == "signup" { 2 } else { 1 });
        for (email, subject, body) in delivered {
            if purpose == "signup" && email == "known@example.com" {
                assert!(body.contains("你已有账号，请直接登录或找回密码"));
                assert!(!body.contains("验证码"));
                assert!(!subject.contains("验证码"));
            } else {
                assert!(body.contains(match purpose { "signup" => "注册账号", "login" => "登录", _ => "找回密码" }));
                let code = subject.split_whitespace().last().ok_or("no code")?;
                assert_eq!(code.len(), 6);
                mail::verify_code(&email, purpose, code)?;
                assert_eq!(mail::verify_code(&email, purpose, code), Err(mail::INVALID_CODE.into()));
            }
        }
        for email in ["known@example.com", "new@example.com"] {
            let count: i64 = db::open_db()?.query_row(
                "SELECT count(*) FROM email_codes WHERE email=?1 AND purpose=?2",
                [email, purpose], |r| r.get(0),
            ).map_err(|e| e.to_string())?;
            assert_eq!(count, 0);
        }
        let before = letters.lock().len();
        let known_retry = send("known@example.com", purpose, letters.clone()).await?;
        let new_retry = send("new@example.com", purpose, letters.clone()).await?;
        assert_eq!(known_retry, new_retry);
        assert!(!known_retry.0.is_success());
        assert_eq!(letters.lock().len(), before, "rate limit includes reminders and no-mail requests");
    }
    Ok(())
}

#[tokio::test]
async fn authority_signup_and_reset_also_hide_code_and_account_state() -> Result<(), String> {
    let _fixture = Fixture::new()?;
    std::env::set_var("POLARIS_ACCOUNT_AUTHORITY", "1");
    std::env::set_var("POLARIS_ACCOUNT_OPEN_SIGNUP", "1");
    let phc = auth::hash_password("123456")?;
    for purpose in ["signup", "reset"] {
        plant("known@example.com", purpose, &phc, 600, 0)?;
        let mut responses = vec![];
        for email in ["known@example.com", "new@example.com"] {
            let request = Json(json!({"email":email,"code":"000000"}));
            let response = if purpose == "signup" {
                account_signup(request).await
            } else {
                account_reset(request).await
            };
            responses.push(wire(response).await?);
        }
        assert_eq!(responses[0], responses[1]);
        assert_eq!(serde_json::from_slice::<Value>(&responses[0].1).map_err(|e| e.to_string())?,
            json!({"error":mail::INVALID_CODE}));
    }
    plant("new@example.com", "reset", &phc, 600, 0)?;
    let response = wire(account_reset(Json(json!({"email":"new@example.com","code":"123456"}))).await).await?;
    assert_eq!(serde_json::from_slice::<Value>(&response.1).map_err(|e| e.to_string())?,
        json!({"error":mail::INVALID_CODE}));
    Ok(())
}

#[tokio::test]
async fn response_does_not_wait_for_smtp_or_expose_delivery_failure() -> Result<(), String> {
    let _fixture = Fixture::new()?;
    for (email, purpose) in [("known@example.com", "signup"), ("new@example.com", "signup"),
        ("known@example.com", "reset"), ("known@example.com", "login")] {
        clear_rates()?;
        let (release, wait) = std::sync::mpsc::channel();
        let (response, task) = tokio::time::timeout(Duration::from_secs(2), send_code_with(
            "unknown-peer".into(), json!({"email":email,"purpose":purpose}),
            mail::delivery::Pool::configured()?, move |_, _, _, _| {
                wait.recv_timeout(Duration::from_secs(5)).map_err(|e| e.to_string())?;
                Err("fake SMTP failure".into())
            },
        )).await.map_err(|e| e.to_string())?;
        let result = wire(response).await?;
        release.send(()).map_err(|e| e.to_string())?;
        task.ok_or("no background task")?.await.map_err(|e| e.to_string())?;
        assert_eq!(result, (StatusCode::OK, br#"{"ok":true}"#.to_vec()));
        assert_eq!(mail::verify_code(email, purpose, "123456"), Err(mail::INVALID_CODE.into()));
    }
    Ok(())
}

#[tokio::test]
async fn submitting_codes_has_one_error_for_missing_wrong_expired_and_exhausted_codes() -> Result<(), String> {
    let _fixture = Fixture::new()?;
    let phc = auth::hash_password("123456")?;
    let mut expected = None;
    for purpose in ["signup", "reset", "login"] {
        for (ttl, attempts, code) in [(600, 0, "000000"), (-1, 0, "123456"),
            (600, 5, "123456"), (600, 0, "")] {
            plant("known@example.com", purpose, &phc, ttl, attempts)?;
            for email in ["known@example.com", "new@example.com"] {
                let request = Json(json!({"email":email,"code":code,"username":"newuser",
                    "password":"test-password-123","newPassword":"test-password-123"}));
                let response = match purpose {
                    "signup" => collab_email_signup(request).await,
                    "reset" => collab_email_reset(request).await,
                    _ => collab_email_login(request).await,
                };
                let actual = wire(response).await?;
                assert!(!actual.0.is_success());
                assert_eq!(serde_json::from_slice::<Value>(&actual.1).map_err(|e| e.to_string())?,
                    json!({"error":mail::INVALID_CODE}));
                if let Some(expected) = &expected { assert_eq!(&actual, expected); }
                else { expected = Some(actual); }
            }
        }
    }
    // Stale valid codes cannot disclose later account binding/deletion either.
    plant("known@example.com", "signup", &phc, 600, 0)?;
    assert_eq!(mail::verify_code("known@example.com", "signup", "123456"), Err(mail::INVALID_CODE.into()));
    for purpose in ["reset", "login"] {
        plant("new@example.com", purpose, &phc, 600, 0)?;
        let request = Json(json!({"email":"new@example.com","code":"123456","newPassword":"test-password-123"}));
        let response = if purpose == "reset" { collab_email_reset(request).await } else { collab_email_login(request).await };
        assert_eq!(Some(wire(response).await?), expected);
    }
    Ok(())
}

#[tokio::test]
async fn reminder_and_no_mail_requests_keep_hourly_and_ip_limits() -> Result<(), String> {
    let _fixture = Fixture::new()?;
    for (email, purpose) in [("known@example.com", "signup"), ("new@example.com", "reset")] {
        for _ in 0..5 {
            mail::CodeRequest::prepare(email, purpose, "")?.deliver(|_, _, _| Ok(()))?;
            db::open_db()?.execute("UPDATE login_attempts SET last_at=?1 WHERE scope='email' AND key=?2",
                rusqlite::params![db::now() - 61, email]).map_err(|e| e.to_string())?;
        }
        assert!(matches!(mail::CodeRequest::prepare(email, purpose, ""), Err(e) if e.contains("一小时")));
    }
    clear_rates()?;
    for i in 0..20 {
        mail::CodeRequest::prepare(&format!("absent{i}@example.com"), "login", "192.0.2.1")?
            .deliver(|_, _, _| Err("must not send".into()))?;
    }
    assert!(matches!(mail::CodeRequest::prepare("last@example.com", "login", "192.0.2.1"),
        Err(e) if e.contains("网络")));
    Ok(())
}

#[tokio::test]
async fn public_handler_keeps_configuration_and_input_errors() -> Result<(), String> {
    let _fixture = Fixture::new()?;
    for request in [json!({"email":"known@example.com","purpose":"invalid"}),
        json!({"email":"bad","purpose":"signup"})] {
        assert!(!collab_email_send_code(None, HeaderMap::new(), Json(request)).await.status().is_success());
    }
    db::meta_set("email_signup", "0")?;
    let closed = collab_email_send_code(None, HeaderMap::new(), Json(json!({"email":"new@example.com","purpose":"signup"}))).await;
    assert!(!closed.status().is_success());
    db::meta_set("smtp_pass", "")?;
    for purpose in ["signup", "reset", "login"] {
        let known = wire(collab_email_send_code(None, HeaderMap::new(), Json(json!({"email":"known@example.com","purpose":purpose}))).await).await?;
        let unknown = wire(collab_email_send_code(None, HeaderMap::new(), Json(json!({"email":"new@example.com","purpose":purpose}))).await).await?;
        assert_eq!(known, unknown);
        assert!(!known.0.is_success());
    }
    Ok(())
}

#[tokio::test]
async fn queue_full_response_is_identical_for_every_account_state() -> Result<(), String> {
    let _fixture = Fixture::new()?;
    let pool = mail::delivery::Pool::new(&crate::collab::interconnect::hardening::MailDelivery {
        capacity: 1, concurrency: 1, timeout_secs: 10,
    })?;
    let (release, wait) = std::sync::mpsc::channel();
    let held = pool.submit(move |_| {
        wait.recv_timeout(Duration::from_secs(5)).map_err(|_| mail::delivery::Stage::Smtp)?;
        Ok(())
    }).ok_or("no slot")?;
    for purpose in ["signup", "reset", "login"] {
        clear_rates()?;
        for email in ["known@example.com", "new@example.com"] {
            let (response, task) = send_code_with("192.0.2.1".into(),
                json!({"email":email,"purpose":purpose}), &pool,
                |_, _, _, _| Err("must not send".into())).await;
            assert!(task.is_none());
            assert_eq!(wire(response).await?, (StatusCode::OK, br#"{"ok":true}"#.to_vec()));
        }
    }
    release.send(()).map_err(|e| e.to_string())?;
    held.await.map_err(|e| e.to_string())?;
    Ok(())
}

#[tokio::test]
async fn expired_smtp_does_not_store_a_code_and_releases_capacity() -> Result<(), String> {
    let _fixture = Fixture::new()?;
    let pool = mail::delivery::Pool::new(&crate::collab::interconnect::hardening::MailDelivery {
        capacity: 1, concurrency: 1, timeout_secs: 1,
    })?;
    let (response, task) = send_code_with("192.0.2.1".into(),
        json!({"email":"new@example.com","purpose":"signup"}), &pool,
        |_, _, _, remaining| { std::thread::sleep(remaining + Duration::from_millis(30)); Ok(()) }).await;
    assert_eq!(wire(response).await?, (StatusCode::OK, br#"{"ok":true}"#.to_vec()));
    task.ok_or("no task")?.await.map_err(|e| e.to_string())?;
    let count: i64 = db::open_db()?.query_row("SELECT count(*) FROM email_codes", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    assert_eq!(count, 0);
    pool.submit(|_| Ok(())).ok_or("slot leaked after timeout")?.await.map_err(|e| e.to_string())?;
    Ok(())
}

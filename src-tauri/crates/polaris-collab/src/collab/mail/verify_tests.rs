use super::*;
use crate::collab::interconnect::test_env::TestEnv;
use std::sync::{Arc, Barrier};

fn plant() -> Result<(), String> {
    open_db()?.execute(
        "INSERT INTO email_codes(email,purpose,code_hash,attempts,created_at,expires_at)
         VALUES('atomic@example.com','login',?1,0,?2,?3)",
        params![auth::hash_password("123456")?, now(), now() + 600],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

fn concurrent(code: &'static str) -> Result<Vec<Result<(), String>>, String> {
    let barrier = Arc::new(Barrier::new(8));
    let threads: Vec<_> = (0..8).map(|_| {
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            barrier.wait();
            verify_code("atomic@example.com", "login", code)
        })
    }).collect();
    threads.into_iter().map(|t| t.join().map_err(|_| "worker panicked".into())).collect()
}

#[test]
fn concurrent_valid_code_has_exactly_one_consumer() -> Result<(), String> {
    let _env = TestEnv::new("atomic-valid")?;
    plant()?;
    let results = concurrent("123456")?;
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert!(results.iter().filter_map(|r| r.as_ref().err()).all(|e| e == INVALID_CODE));
    Ok(())
}

#[test]
fn concurrent_wrong_codes_cannot_exceed_attempt_limit() -> Result<(), String> {
    let _env = TestEnv::new("atomic-wrong")?;
    plant()?;
    assert!(concurrent("000000")?.iter().all(|r| r == &Err(INVALID_CODE.into())));
    let attempts: i64 = open_db()?.query_row("SELECT attempts FROM email_codes", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    assert_eq!(attempts, MAX_ATTEMPTS);
    assert_eq!(verify_code("atomic@example.com", "login", "123456"), Err(INVALID_CODE.into()));
    Ok(())
}

#[test]
fn write_errors_are_returned_and_consumption_rolls_back() -> Result<(), String> {
    let _env = TestEnv::new("atomic-errors")?;
    plant()?;
    for operation in ["UPDATE", "DELETE"] {
        open_db()?.execute_batch(&format!(
            "CREATE TRIGGER fail_write BEFORE {operation} ON email_codes
             BEGIN SELECT RAISE(ABORT, 'injected write failure'); END;"
        )).map_err(|e| e.to_string())?;
        let result = verify_code("atomic@example.com", "login", "123456");
        assert!(matches!(result, Err(e) if e.contains("injected write failure")));
        let attempts: i64 = open_db()?.query_row("SELECT attempts FROM email_codes", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        assert_eq!(attempts, 0);
        open_db()?.execute_batch("DROP TRIGGER fail_write").map_err(|e| e.to_string())?;
    }
    verify_code("atomic@example.com", "login", "123456")
}

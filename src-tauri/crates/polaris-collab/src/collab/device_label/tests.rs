use super::validate_characters;
use crate::collab::{auth, authority, db, grants, identity, interconnect, mesh};
use interconnect::test_env::TestEnv;
use serde_json::json;

#[test]
fn browser_labels_preserve_unicode_limits_and_reject_delimiters() -> Result<(), String> {
    for field in ["deviceName", "platform"] {
        for invalid in ["<", ">", "\"", "'", "`", "\n", "\0", "\u{7f}"] {
            let input = json!({"browser":{field:format!("设备{invalid}名称")}}).to_string();
            assert!(interconnect::resolve(Some(&input), |_| None).is_err(), "{input}");
        }
        for name in ["书房电脑-01_（测试） & + / . : @ # 😀".to_string(), "中".repeat(40)] {
            let input = json!({"browser":{field:name}}).to_string();
            interconnect::resolve(Some(&input), |_| None)?;
        }
        for name in [String::new(), "   ".into(), "中".repeat(41)] {
            let input = json!({"browser":{field:name}}).to_string();
            assert!(interconnect::resolve(Some(&input), |_| None).is_err());
        }
    }
    Ok(())
}

#[test]
fn all_device_name_writers_reject_before_mutating_storage() -> Result<(), String> {
    let _env = TestEnv::new("device-label-writers")?;
    let user = auth::create_user("labeluser", "test-password-123", "member", "成员")?;
    let ticket = identity::create_ticket("visitor", "")?;
    let key = mesh::enroll("acct", "node", "中文电脑", "linux", "1")?;
    identity::add_device(user.id, "中文电脑", "node")?;
    grants::grant_by_code("node", "中文电脑")?;
    for bad in ['<', '>', '"', '\'', '`', '\n', '\0', '\u{7f}'] {
        let name = format!("设备{bad}名称");
        assert!(validate_characters(&name).is_err());
        assert!(mesh::enroll("acct", "node", &name, "linux", "1").is_err());
        assert!(mesh::rename("acct", "node", &name).is_err());
        assert!(mesh::announce(&key, &name, "linux", "1").is_err());
        assert!(identity::add_device(user.id, &name, "new-node").is_err());
        assert!(identity::ensure_device_for(user.id, "node", &name).is_err());
        assert!(identity::ensure_device_for(user.id, "new-node", &name).is_err());
        assert!(identity::redeem_ticket(&ticket.code, "newlabeluser", "test-password-123",
            "成员", &name, "new-node").is_err());
        assert!(identity::redeem_ticket_existing(&ticket.code, &user, &name, "new-node").is_err());
        assert!(grants::grant_by_code("node", &name).is_err());
        assert!(grants::grant_by_code("new-node", &name).is_err());
        assert!(grants::auto_grant_self("node", "acct", &name, false).is_err());
        assert!(grants::auto_grant_self("new-node", "acct", &name, false).is_err());
        assert!(grants::update("node", Some(&name), None, None, None, "owner").is_err());
        assert!(authority::join_with_ticket("", &ticket.code, &name, "new-node")
            .is_err_and(|e| e.contains("设备名称")));
    }
    assert_eq!(mesh::devices("acct")?.len(), 1);
    assert_eq!(mesh::devices("acct")?[0].0.name, "中文电脑");
    assert_eq!(identity::list_devices()?.len(), 1);
    assert_eq!(grants::list()?.len(), 1);
    assert_eq!(grants::get("node")?.ok_or("missing grant")?.name, "中文电脑");
    let used: Option<i64> = db::with_conn(|conn| conn.query_row(
        "SELECT used_at FROM tickets WHERE code=?1", [&ticket.code], |r| r.get(0))
        .map_err(|e| e.to_string()))?;
    assert_eq!(used, None, "invalid names must not consume an invitation");
    Ok(())
}

#[test]
fn unicode_names_and_existing_length_rules_survive() -> Result<(), String> {
    let _env = TestEnv::new("device-label-lengths")?;
    let name = "书房电脑-01_（测试） & + / . : @ # 😀";
    let user = auth::create_user("labeluser", "test-password-123", "member", "成员")?;
    let long = "中".repeat(81);
    let key = mesh::enroll("acct", "node", &long, "linux", "1")?;
    assert_eq!(mesh::devices("acct")?[0].0.name, long);
    mesh::announce(&key, name, "linux", "1")?;
    assert_eq!(mesh::devices("acct")?[0].0.name, name);
    mesh::rename("acct", "node", &"中".repeat(40))?;
    assert!(mesh::rename("acct", "node", &"中".repeat(41)).is_err());
    mesh::rename("acct", "node", name)?;
    mesh::announce(&key, "", "", "")?;
    assert_eq!(mesh::devices("acct")?[0].0.name, name);
    identity::add_device(user.id, name, "named-node")?;
    identity::ensure_device_for(user.id, "long-node", &long)?;
    assert!(identity::list_devices()?.iter().any(|device| device.name == name));
    assert!(identity::list_devices()?.iter().any(|device| device.name == long));
    grants::grant_by_code("node", name)?;
    assert_eq!(grants::update("node", Some(&long), None, None, None, "owner")?.name, long);
    assert_eq!(grants::auto_grant_self("self-node", "acct", name, false)?.0.name, name);
    let ticket = identity::create_ticket("visitor", "")?;
    assert!(identity::redeem_ticket_existing(&ticket.code, &user, &long, "ticket-node").is_err());
    identity::redeem_ticket_existing(&ticket.code, &user, &"中".repeat(80), "ticket-node")?;
    let ticket = identity::create_ticket("visitor", "")?;
    assert!(identity::redeem_ticket(&ticket.code, "newlabeluser", "test-password-123",
        "成员", &long, "new-node").is_err());
    identity::redeem_ticket(&ticket.code, "newlabeluser", "test-password-123",
        "成员", &"中".repeat(80), "new-node")?;
    Ok(())
}

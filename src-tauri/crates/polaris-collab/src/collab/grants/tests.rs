use super::*;

fn tmp_db(tag: &str) -> std::sync::MutexGuard<'static, ()> {
    let g = db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let p = std::env::temp_dir().join(format!("grants-test-{}-{tag}.db", std::process::id()));
    let _ = std::fs::remove_file(&p);
    std::env::set_var("POLARIS_COLLAB_DB", &p);
    g
}

/// 策略 B 的第一台和后续设备均立即全权。
#[test]
fn both_first_and_second_devices_have_full_contract_permissions() -> Result<(), String> {
    let _g = tmp_db("same-account");
    let (first, fresh) = auto_grant_self("node-a", "acct_1", "我的电脑", true)?;
    assert!(fresh);
    assert_eq!(first.effective_fs(), FS_RW);
    assert_eq!(first.effective_exec(), EXEC_ALLOW);
    assert!(first.should_mount());

    let (second, _) = auto_grant_self("node-b", "acct_1", "NAS", false)?;
    assert_eq!(second.fs_access, FS_RW, "库里记的仍是用户批的档");
    assert_eq!(second.effective_fs(), FS_RW);
    assert_eq!(second.effective_exec(), EXEC_ALLOW);
    assert!(second.should_mount(), "全权设备应自动挂载");
    Ok(())
}

/// 重连不该把用户手工调过的档位冲回默认值 —— 「每次重启权限重置一遍」正是要修的毛病。
#[test]
fn reconnect_does_not_reset_user_choice() -> Result<(), String> {
    let _g = tmp_db("keep");
    auto_grant_self("node-a", "acct_1", "NAS", true)?;
    update(
        "node-a",
        None,
        Some(FS_RO),
        Some(EXEC_NONE),
        Some(false),
        "wuli",
    )
    ?;

    let (again, fresh) = auto_grant_self("node-a", "acct_1", "NAS", true)?;
    assert!(!fresh, "已有契约不该被当成新设备");
    assert_eq!(again.fs_access, FS_RO);
    assert_eq!(again.exec_access, EXEC_NONE);
    assert!(!again.auto_mount);
    assert!(!again.should_mount(), "用户关掉自动挂载后就不该再自动挂");
    Ok(())
}

/// 撤销 = 入方向直接拒 + 出方向不再挂。没批过的设备不算被撤销(它走别的门)。
#[test]
fn revoke_denies_both_directions() -> Result<(), String> {
    let _g = tmp_db("revoke");
    auto_grant_self("node-a", "acct_1", "旧电脑", true)?;
    assert!(!is_revoked("node-a"));

    revoke("node-a", "wuli")?;
    assert!(is_revoked("node-a"));
    let g = get("node-a")?.ok_or("missing grant")?;
    assert_eq!(g.effective_fs(), FS_NONE);
    assert_eq!(g.effective_exec(), EXEC_NONE);
    assert!(!g.should_mount());

    // 没批过的设备:不是「被撤销」,别越权把正常首登堵死。
    assert!(!is_revoked("node-never-seen"));
    // 撤销没见过的设备也得成功(丢电脑时用户不该被「它还没登记」挡住)。
    revoke("node-ghost", "wuli")?;
    assert!(is_revoked("node-ghost"));
    Ok(())
}

#[test]
fn bad_levels_are_refused() -> Result<(), String> {
    let _g = tmp_db("bad");
    auto_grant_self("node-a", "acct_1", "机器", true)?;
    assert!(update("node-a", None, Some("everything"), None, None, "x").is_err());
    assert!(update("node-a", None, None, Some("root"), None, "x").is_err());
    // 改不存在的设备要报错,不能静默建一条出来。
    assert!(update("node-zzz", None, Some(FS_RO), None, None, "x").is_err());
    Ok(())
}

#[test]
fn drive_hint_survives() -> Result<(), String> {
    let _g = tmp_db("drive");
    auto_grant_self("node-a", "acct_1", "NAS", true)?;
    remember_drive("node-a", "Y:")?;
    assert_eq!(get("node-a")?.ok_or("missing grant")?.drive_hint, "Y:");
    assert!(any_granted()?);
    Ok(())
}

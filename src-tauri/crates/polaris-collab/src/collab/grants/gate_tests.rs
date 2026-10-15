use super::*;

fn tmp(tag: &str) -> std::sync::MutexGuard<'static, ()> {
    let g = db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var(
        "POLARIS_COLLAB_DB",
        std::env::temp_dir().join(format!(
            "grants-gate-{tag}-{}-{}.db",
            std::process::id(),
            now()
        )),
    );
    g
}

/// 设备码进来的对端:**默认只读、禁执行**。这是用户拍板的那一档。
#[test]
fn code_peer_is_read_only_until_trusted() -> Result<(), String> {
    let _g = tmp("code");
    let peer = "node-someone-else";
    let created = grant_by_code(peer, "小李的笔记本")?;
    assert_eq!(created.fs_access, FS_RO);
    assert_eq!(created.exec_access, EXEC_NONE);
    assert!(!created.auto_mount, "别人的设备不该自动挂进我的盘符");

    assert!(fs_gate(peer, false).is_ok(), "读要放行");
    let e = fs_gate(peer, true).err().ok_or("expected error")?;
    assert!(e.contains("只读"), "写必须被挡住,err={e}");
    assert!(exec_gate(peer).err().ok_or("expected error")?.contains("禁止"));

    // 机主点「信任这台」→ 一步升到全权。
    trust_fully(peer, "本机")?;
    assert!(fs_gate(peer, true).is_ok());
    assert!(exec_gate(peer).is_ok());
    Ok(())
}

/// 重连不能把机主调过的档冲回默认值 —— 那等于「每次重连权限重置一遍」。
#[test]
fn reconnect_keeps_the_owner_decision() -> Result<(), String> {
    let _g = tmp("keep");
    grant_by_code("node-p", "对方")?;
    trust_fully("node-p", "本机")?;
    let again = grant_by_code("node-p", "对方")?;
    assert_eq!(again.fs_access, FS_RW, "重连不该把 rw 冲回 ro");
    // 反过来也一样:降成不挂盘之后,对方重连仍是不挂盘。
    update("node-p", None, Some(FS_NONE), None, None, "本机")?;
    assert_eq!(grant_by_code("node-p", "对方")?.fs_access, FS_NONE);
    assert!(fs_gate("node-p", false).err().ok_or("expected error")?.contains("不挂盘"));
    Ok(())
}

/// 踢掉的设备拿着旧码回来也进不去。
#[test]
fn revoked_peer_cannot_reconnect_with_the_code() -> Result<(), String> {
    let _g = tmp("revoked");
    grant_by_code("node-bad", "谁")?;
    revoke("node-bad", "本机")?;
    assert!(grant_by_code("node-bad", "谁")
        .err().ok_or("expected error")?
        .contains("移出"));
    assert!(fs_gate("node-bad", false).is_err());
    assert!(exec_gate("node-bad").is_err());
    Ok(())
}

/// 本机自己不受契约约束:桌面会话上记的是本机 NodeId,而本机也给自己建了一行契约。
#[test]
fn my_own_machine_is_never_gated_by_its_own_grant() -> Result<(), String> {
    let _g = tmp("self");
    db::meta_set("host_node_id", "node-me")?;
    // 造一行最严的契约扣在自己头上。
    grant_by_code("node-me", "我自己")?;
    update(
        "node-me",
        None,
        Some(FS_NONE),
        Some(EXEC_NONE),
        None,
        "本机",
    )
    ?;
    assert!(fs_gate("node-me", true).is_ok(), "本机不该被自己的契约锁住");
    assert!(exec_gate("node-me").is_ok());
    // 空 device_id(全局口令 / 老会话 / 本机内部调用)同样豁免。
    assert!(fs_gate("", true).is_ok());
    assert!(exec_gate("").is_ok());
    Ok(())
}

/// 没有契约的设备不额外收紧 —— 老手机壳粘 PLRK1 那条路仍旧只过角色闸。
#[test]
fn unknown_device_falls_back_to_the_role_gate() -> Result<(), String> {
    let _g = tmp("unknown");
    db::meta_set("host_node_id", "node-me")?;
    assert!(fs_gate("node-never-seen", true).is_ok());
    assert!(exec_gate("node-never-seen").is_ok());
    Ok(())
}

/// 策略 B 的同账号新设备在入方向立即可读写、可执行。
#[test]
fn same_account_new_device_can_read_write_and_execute() -> Result<(), String> {
    let _g = tmp("same-account");
    db::meta_set("host_node_id", "node-me")?;
    auto_grant_self("node-first", "acct_1", "第一台", true)?;
    auto_grant_self("node-2nd", "acct_1", "新来的", false)?;
    assert!(fs_gate("node-2nd", false).is_ok(), "读照旧");
    assert!(fs_gate("node-2nd", true).is_ok());
    assert!(exec_gate("node-2nd").is_ok());
    // 第一台也立即全权。
    assert!(fs_gate("node-first", true).is_ok());
    assert!(exec_gate("node-first").is_ok());
    Ok(())
}

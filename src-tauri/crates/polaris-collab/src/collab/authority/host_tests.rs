use super::*;
use super::tests::tmp_env;

/// 本次改动要证明的那件事:**一个账号,在两台互不相识的主机上都能登进去**。
///
/// 两台主机 = 两个 collab.db(切 `POLARIS_COLLAB_DB` 就是换一台机器的视角:同一份代码、
/// 另一份数据)。权威机建号签断言,两台成员主机各自**离线验签**、各自落自己的成员资格 ——
/// 全程没有任何一台成员主机见过密码,也没有任何一台需要连得上权威。

#[test]
fn one_account_logs_into_two_independent_hosts() {
    let _g = tmp_env();
    let stamp = std::process::id();
    let db_authority = std::env::temp_dir().join(format!("auth-cloud-{stamp}.db"));
    let db_host_a = std::env::temp_dir().join(format!("auth-hostA-{stamp}.db"));
    let db_host_b = std::env::temp_dir().join(format!("auth-hostB-{stamp}.db"));
    for p in [&db_authority, &db_host_a, &db_host_b] {
        let _ = std::fs::remove_file(p);
    }

    // ── 云机(账号权威):建全局账号 + 签断言 ──
    std::env::set_var("POLARIS_COLLAB_DB", &db_authority);
    let pubkey = public_key_b64().unwrap();
    let (acct, uid) = crate::collab::auth::create_authority_account(
        "wuli",
        "s3cret-8",
        "武力",
        "wuli@example.com",
    )
    .unwrap();
    assert!(!uid.is_empty());
    // 权威侧用**邮箱**也登得进(注册绑的就是它)。
    let (_, uid2, email) =
        crate::collab::auth::authority_login("wuli@example.com", "s3cret-8").unwrap();
    assert_eq!(uid2, uid, "邮箱登录与用户名登录必须是同一个全局身份");
    assert_eq!(email, "wuli@example.com");
    let assertion = sign_assertion(&uid, &acct.username, &email, "武力").unwrap();

    // ── 主机 A(比如家里的桌面):空库,离线验签即进,首个成员当 owner ──
    std::env::set_var("POLARIS_COLLAB_DB", &db_host_a);
    let claims_a = verify_assertion_with(&assertion, &pubkey).unwrap();
    let ua = upsert_member(&claims_a, Admission::ExistingOnly).unwrap();
    assert_eq!(ua.username, "wuli");
    assert_eq!(ua.role, "owner");
    let token_a = crate::collab::auth::issue_session(ua.id, "devA").unwrap();
    assert_eq!(
        crate::collab::auth::check_session(&token_a)
            .unwrap()
            .username,
        "wuli"
    );

    // ── 主机 B(比如 NAS):另一座库,同一张断言照样进 ──
    std::env::set_var("POLARIS_COLLAB_DB", &db_host_b);
    let claims_b = verify_assertion_with(&assertion, &pubkey).unwrap();
    let ub = upsert_member(&claims_b, Admission::ExistingOnly).unwrap();
    assert_eq!(ub.username, "wuli");
    let token_b = crate::collab::auth::issue_session(ub.id, "devB").unwrap();

    // 全局身份同一个;两台机器上的本地行号与会话各自独立(互不影响,互不泄露)。
    assert_eq!(claims_a.uid, claims_b.uid, "跨主机认的必须是同一个 uid");
    assert!(
        crate::collab::auth::check_session(&token_a).is_err(),
        "主机 A 的会话不该在主机 B 上有效"
    );
    assert_eq!(
        crate::collab::auth::check_session(&token_b)
            .unwrap()
            .username,
        "wuli"
    );

    // 密码从没离开过权威:成员主机上拿真密码也登不进去。
    assert!(
        crate::collab::auth::login("wuli", "s3cret-8", "devB").is_err(),
        "联邦账号不该能在成员主机上用密码登录"
    );

    for p in [&db_authority, &db_host_a, &db_host_b] {
        let _ = std::fs::remove_file(p);
    }
}

#[test]
fn username_squatting_is_refused() {
    let _g = tmp_env();
    // 本机先有一个同名的本地账号(uid 为空)。
    crate::collab::auth::create_user("alice", "s3cret-8", "owner", "Local Alice").unwrap();
    let c = Claims {
        iss: "k".into(),
        uid: "acct_1".into(),
        username: "alice".into(),
        email: "a@b.com".into(),
        display_name: "Alice".into(),
        device_id: None,
        node_id: None,
        iat: now(),
        exp: now() + 300,
    };
    let e = upsert_member(&c, Admission::ExistingOnly).unwrap_err();
    assert!(e.contains("同名"), "err={e}");
}

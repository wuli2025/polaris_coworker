use super::*;

/// 独占一个临时库 + 临时签名密钥(两者都是进程级环境变量,必须串行)。
pub(super) fn tmp_env() -> std::sync::MutexGuard<'static, ()> {
    let g = crate::collab::db::TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let stamp = format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    std::env::set_var(
        "POLARIS_COLLAB_DB",
        std::env::temp_dir().join(format!("authority-test-{stamp}.db")),
    );
    std::env::set_var(
        "POLARIS_ACCOUNT_KEY",
        std::env::temp_dir().join(format!("authority-test-{stamp}.key")),
    );
    std::env::remove_var("POLARIS_ACCOUNT_AUTHORITY");
    std::env::remove_var("POLARIS_ACCOUNT_AUTHORITY_URL");
    g
}

#[test]
fn sign_then_verify_roundtrip() {
    let _g = tmp_env();
    let pk = public_key_b64().unwrap();
    let a = sign_assertion("acct_x", "alice", "a@b.com", "Alice").unwrap();
    let c = verify_assertion_with(&a, &pk).unwrap();
    assert_eq!(c.uid, "acct_x");
    assert_eq!(c.username, "alice");
    assert_eq!(c.email, "a@b.com");
}

#[test]
fn tampered_assertion_is_rejected() {
    let _g = tmp_env();
    let pk = public_key_b64().unwrap();
    let a = sign_assertion("acct_x", "alice", "a@b.com", "Alice").unwrap();
    // 改载荷(把 alice 换成 root)后重新拼装 —— 签名覆盖载荷,必须验不过。
    let mut parts: Vec<&str> = a.split('.').collect();
    let raw = B64.decode(parts[1]).unwrap();
    let mut claims: Claims = serde_json::from_slice(&raw).unwrap();
    claims.username = "root".into();
    let forged = B64.encode(serde_json::to_vec(&claims).unwrap());
    parts[1] = &forged;
    let bad = parts.join(".");
    assert!(verify_assertion_with(&bad, &pk).is_err());
}

#[test]
fn other_key_cannot_verify() {
    let _g = tmp_env();
    let a = sign_assertion("acct_x", "alice", "a@b.com", "Alice").unwrap();
    // 另一把钥匙的公钥:验不过(这就是「换把钥匙冒充权威」被挡住的那一刻)。
    let mut seed = [7u8; 32];
    seed[0] = 9;
    let other = SigningKey::from_bytes(&seed);
    let other_pub = B64.encode(other.verifying_key().to_bytes());
    assert!(verify_assertion_with(&a, &other_pub).is_err());
}

#[test]
fn expired_assertion_is_rejected() {
    let _g = tmp_env();
    let pk = public_key_b64().unwrap();
    let sk = signing_key().unwrap();
    let claims = Claims {
        iss: kid_of(&pk),
        uid: "acct_x".into(),
        username: "alice".into(),
        email: "a@b.com".into(),
        display_name: "Alice".into(),
        device_id: None,
        node_id: None,
        iat: now() - 10_000,
        exp: now() - 9_000,
    };
    let head = format!("PA1.{}", B64.encode(serde_json::to_vec(&claims).unwrap()));
    let sig = sk.sign(head.as_bytes());
    let a = format!("{head}.{}", B64.encode(sig.to_bytes()));
    let e = verify_assertion_with(&a, &pk).unwrap_err();
    assert!(e.contains("过期"), "err={e}");
}

pub(super) fn claims_of(uid: &str, username: &str, email: &str) -> Claims {
    Claims {
        iss: "k".into(),
        uid: uid.into(),
        username: username.into(),
        email: email.into(),
        display_name: username.into(),
        device_id: None,
        node_id: None,
        iat: now(),
        exp: now() + 300,
    }
}

#[test]
fn first_member_becomes_owner_and_is_idempotent() {
    let _g = tmp_env();
    let c1 = claims_of("acct_1", "alice", "a@b.com");
    let u1 = upsert_member(&c1, Admission::ExistingOnly).unwrap();
    assert_eq!(u1.role, "owner", "空库第一个联邦账号应成为本机 owner");

    // 幂等:同 uid 再来一次不新建、不降级。
    let again = upsert_member(&c1, Admission::ExistingOnly).unwrap();
    assert_eq!(again.id, u1.id);
    assert_eq!(again.role, "owner");
}

/// 这次改动的安全线:**云端有账号 ≠ 本机欢迎你**。
/// 主机已有主人之后,陌生的云端账号不带邀请码一律进不来 —— 否则谁在云端注册
/// 一个号就能登进别人家的主机,而主机上的成员能触发构建脚本 = 主机 RCE。
#[test]
fn stranger_with_valid_identity_is_not_auto_admitted() {
    let _g = tmp_env();
    upsert_member(
        &claims_of("acct_1", "alice", "a@b.com"),
        Admission::ExistingOnly,
    )
    .unwrap();

    let stranger = claims_of("acct_2", "mallory", "m@b.com");
    let e = upsert_member(&stranger, Admission::ExistingOnly).unwrap_err();
    assert!(e.contains("设备码"), "err={e}");

    // 带票据角色才进得来,且角色由票据说了算(不是自封)。
    let u = upsert_member(&stranger, Admission::Invited("visitor".into())).unwrap();
    assert_eq!(u.role, "visitor");
    // 进来之后就是成员,日常登录不再需要票据。
    assert_eq!(
        upsert_member(&stranger, Admission::ExistingOnly)
            .unwrap()
            .role,
        "visitor"
    );
}

/// 本次改动的**主线**:同一个账号的第二台设备,**零邀请码**就该进得去。
/// 这正是用户说的「不要还得登两个账号」——它在代码里就是这一个断言。
#[test]
fn my_own_second_device_needs_no_invite_code() {
    let _g = tmp_env();
    // 第一台设备上首登 → 成 owner,同时把 owner_uid 钉死。
    let me = claims_of("acct_me", "wuli", "wuli@qq.com");
    let u1 = upsert_member(&me, Admission::ExistingOnly).unwrap();
    assert_eq!(u1.role, "owner");
    assert_eq!(owner_uid(), "acct_me", "首个 owner 必须落成本机主人");

    // 模拟「另一台机器」:同一个 uid 拿断言来 —— 走 SelfOwned,不要票据。
    let (u2, token) = login_with_assertion(&fresh_assertion_for(&me), "node-second").unwrap();
    assert_eq!(u2.role, "owner");
    assert!(!token.is_empty());
    // 且它已被登记进信任契约；高风险批准另由 interconnect::gate 管。
    assert!(crate::collab::grants::get("node-second").unwrap().is_some());
}

/// 安全线一:`owner_uid` 只写一次。否则「谁最后登录谁是主人」= 远程夺权。
#[test]
fn owner_uid_cannot_be_hijacked_by_a_later_owner() {
    let _g = tmp_env();
    upsert_member(
        &claims_of("acct_me", "wuli", "w@q.com"),
        Admission::ExistingOnly,
    )
    .unwrap();
    assert_eq!(owner_uid(), "acct_me");

    // 攻击者拿到一张 owner 票据(或本机 owner 手滑给了他 owner 角色)。
    let attacker = claims_of("acct_evil", "mallory", "m@q.com");
    let u = upsert_member(&attacker, Admission::Invited("owner".into())).unwrap();
    assert_eq!(u.role, "owner", "他确实成了本机 owner(票据给的)");
    // 但「本机主人」这个身份没被他抢走 —— 他的设备走不了 SelfOwned 那条免票据的路。
    assert_eq!(owner_uid(), "acct_me", "主人 uid 不该被后来的 owner 覆盖");
}

/// 安全线二:**撤销优先于「我是主人」**。丢了电脑在别处点了「移出」之后,
/// 捡到电脑的人拿着同一个账号也进不来。
#[test]
fn revoked_device_is_refused_even_for_the_owner() {
    let _g = tmp_env();
    let me = claims_of("acct_me", "wuli", "w@q.com");
    upsert_member(&me, Admission::ExistingOnly).unwrap();
    // 先正常进一次,拿到契约。
    login_with_assertion(&fresh_assertion_for(&me), "node-lost").unwrap();
    // 丢了 → 在另一台设备上撤销它。
    crate::collab::grants::revoke("node-lost", "wuli").unwrap();

    let e = login_with_assertion(&fresh_assertion_for(&me), "node-lost").unwrap_err();
    assert!(e.contains("移出"), "err={e}");
    // 别的设备不受影响。
    assert!(login_with_assertion(&fresh_assertion_for(&me), "node-other").is_ok());
}

/// 安全线三:SelfOwned **只**放自己账号。别人的账号照旧要邀请码 —— 这条铁律没松。
#[test]
fn selfowned_does_not_leak_to_other_accounts() {
    let _g = tmp_env();
    upsert_member(
        &claims_of("acct_me", "wuli", "w@q.com"),
        Admission::ExistingOnly,
    )
    .unwrap();
    let stranger = claims_of("acct_other", "mallory", "m@q.com");
    let e = login_with_assertion(&fresh_assertion_for(&stranger), "node-x").unwrap_err();
    assert!(e.contains("设备码"), "陌生账号必须仍被那道码闸挡住,err={e}");
}

/// 用户报的那个「账号主没有邀请你」:v2.9.0 之前装的机器,主人是「设为主机」时建的
/// **本地账号**(没有 uid、没绑邮箱),于是 `owner_uid` 恒空 —— 主人在自己的电脑上
/// 用自己的邮箱登录,反被自己的机器挡在门外。控制台这一档必须把那一行认领下来。
#[test]
fn console_login_adopts_the_legacy_local_owner() {
    let _g = tmp_env();
    // 老机器:一个本地 owner,用户名是当年随手起的,没绑邮箱、没有 uid。
    crate::collab::auth::create_user("wuli2025", "s3cret-8", "owner", "老账号").unwrap();
    assert_eq!(owner_uid(), "", "本地账号不该钉出本机主人");

    let me = claims_of("acct_me", "agdim0003", "agdim0003@outlook.com");
    // 远端递进来的断言照旧被挡(这条铁律没松)。
    let e = upsert_member(&me, Admission::ExistingOnly).unwrap_err();
    assert!(e.contains("设备码"), "err={e}");

    // 本人在这台机器前亲手登录 → 认领**那一行**,不是再建一个「第二个我」。
    let u = upsert_member(&me, Admission::LocalConsole).unwrap();
    assert_eq!(u.role, "owner");
    assert_eq!(u.username, "agdim0003", "用户名统一到账号中心那个");
    assert_eq!(owner_uid(), "acct_me", "认领之后这台机器才算有了主人");
    assert_eq!(
        crate::collab::auth::list_users().unwrap().len(),
        1,
        "认领而不是新建 —— 老账号名下的项目关系要跟着走"
    );
    // 从此走的是 SelfOwned 那条免票据的路。
    assert_eq!(
        upsert_member(&me, Admission::ExistingOnly).unwrap().role,
        "owner"
    );
}

/// 邮箱是硬证据:哪怕这台机器已经有主人、有一堆成员,邮箱对得上的那一行就是同一个人。
/// 它同时是「同一邮箱、用户名不同」那个症状的正解 —— 认人只认邮箱/uid,不认用户名。
#[test]
fn console_login_adopts_by_email_even_on_a_shared_host() {
    let _g = tmp_env();
    upsert_member(
        &claims_of("acct_boss", "boss", "boss@q.com"),
        Admission::ExistingOnly,
    )
    .unwrap();
    assert_eq!(owner_uid(), "acct_boss");
    // 老规矩建的本地成员:邮箱绑了,用户名与账号中心那边不一样。
    crate::collab::auth::create_user_with_email(
        "xiaoli",
        "s3cret-8",
        "collaborator",
        "小李",
        "Li@Example.COM",
    )
    .unwrap();

    let li = claims_of("acct_li", "li", "li@example.com");
    let u = upsert_member(&li, Admission::LocalConsole).unwrap();
    assert_eq!(
        u.role, "collaborator",
        "认领不改角色 —— 不能借登录自封 owner"
    );
    assert_eq!(owner_uid(), "acct_boss", "主人也不该被换掉");
    assert_eq!(crate::collab::auth::list_users().unwrap().len(), 2);
}

/// 控制台这一档不能变成「换个账号登一次就改朝换代」:机器已经有主人、
/// 又没有任何一行认得这个邮箱时,照旧要邀请码。
#[test]
fn console_login_does_not_hijack_a_claimed_host() {
    let _g = tmp_env();
    upsert_member(
        &claims_of("acct_me", "wuli", "w@q.com"),
        Admission::ExistingOnly,
    )
    .unwrap();
    assert_eq!(owner_uid(), "acct_me");

    let stranger = claims_of("acct_evil", "mallory", "m@q.com");
    let e = upsert_member(&stranger, Admission::LocalConsole).unwrap_err();
    assert!(e.contains("设备码"), "err={e}");
    assert_eq!(owner_uid(), "acct_me");
}

/// 邮箱对得上、但那一行已经绑在**另一个**云端账号上:不抢,报清楚。
#[test]
fn console_login_refuses_to_steal_a_row_bound_to_another_account() {
    let _g = tmp_env();
    upsert_member(
        &claims_of("acct_a", "alice", "a@q.com"),
        Admission::ExistingOnly,
    )
    .unwrap();
    // 另一个 uid 拿着同一个邮箱来(正常账号中心不可能签出这种断言,但别人的权威可以)。
    let e = upsert_member(
        &claims_of("acct_b", "bob", "a@q.com"),
        Admission::LocalConsole,
    )
    .unwrap_err();
    assert!(e.contains("另一个云端账号"), "err={e}");
}

/// **一码**:进这台机器的门,用的就是它「互联」页上那串设备码 ——
/// 不再有第二种码。拿它进来的是 visitor(能做什么由设备契约逐台说了算),
/// 绝不是 owner。
#[test]
fn the_device_code_is_the_only_code_needed_to_get_in() {
    let _g = tmp_env();
    // 这台机器已经有主人了 —— 陌生账号照旧进不去。
    let me = claims_of("acct_me", "wuli", "w@q.com");
    upsert_member(&me, Admission::ExistingOnly).unwrap();
    let stranger = claims_of("acct_other", "mallory", "m@q.com");
    let a = fresh_assertion_for(&stranger);
    assert!(login_with_assertion(&a, "node-x").is_err());

    // 拿本机那串设备码来 → 进得去,角色是 visitor。
    let code = crate::collab::identity::access_code().unwrap();
    let (u, token) = join_with_ticket(
        &fresh_assertion_for(&stranger),
        &code,
        "他的笔记本",
        "node-x",
    )
    .unwrap();
    assert_eq!(u.role, "visitor", "设备码永远不该产出一个 owner");
    assert!(!token.is_empty());
    // 设备码是**常驻**的:同一串码换台设备再来一次照样管用(不像票据用一次就废)。
    let other = claims_of("acct_third", "carol", "c@q.com");
    assert_eq!(
        join_with_ticket(
            &fresh_assertion_for(&other),
            &code.to_ascii_lowercase(),
            "第三台",
            "node-y"
        )
        .unwrap()
        .0
        .role,
        "visitor"
    );
    // 换一串之后旧码当场作废。
    crate::collab::identity::rotate_access_code("本机").unwrap();
    let d = claims_of("acct_4th", "dave", "d@q.com");
    assert!(join_with_ticket(&fresh_assertion_for(&d), &code, "第四台", "node-z").is_err());
}

/// 邀请码只有一种,但用户会粘裸码、分享码或互联页 PLR1 设备码。都得认。
#[test]
fn invite_code_accepts_bare_lowercase_share_and_plr1_forms() {
    let _g = tmp_env();
    let share = crate::collab::identity::encode_share_code(
        "ABCD2345",
        &["http://192.168.1.9:8484".to_string()],
    );
    assert_eq!(normalize_invite("ABCD2345"), "ABCD2345");
    assert_eq!(normalize_invite("  abcd2345 "), "ABCD2345");
    assert_eq!(normalize_invite(&share), "ABCD2345");
    assert_eq!(
        normalize_invite("PLR1-ABCD2345-0123456789abcdef"),
        "ABCD2345"
    );
    assert_eq!(
        normalize_invite("plr1-abcd2345-0123456789abcdef"),
        "ABCD2345"
    );
    assert_eq!(normalize_invite("PLR1-ABCD2345"), "PLR1-ABCD2345");
    assert_eq!(normalize_invite("   "), "");
}

/// 用 tmp_env 里的临时签名密钥给一组 claims 现签一张真断言,
/// 并把本机钉在这把钥匙上 —— 让 login_with_assertion 的验签走真路径,不绕过。
fn fresh_assertion_for(c: &Claims) -> String {
    let pk = public_key_b64().unwrap();
    // 首次调用时钉住自己(测试里权威与成员是同一进程、同一把钥匙)。
    if pinned().is_none() {
        db::meta_set("authority_url", "http://test-authority").unwrap();
        db::meta_set("authority_pub", &pk).unwrap();
        db::meta_set("authority_kid", &kid_of(&pk)).unwrap();
    }
    sign_assertion(&c.uid, &c.username, &c.email, &c.display_name).unwrap()
}

/// 一张票据只能用一次:并发/重复入伙不能各拿一个成员资格。
#[test]
fn ticket_is_burned_exactly_once() {
    let _g = tmp_env();
    upsert_member(
        &claims_of("acct_1", "alice", "a@b.com"),
        Admission::ExistingOnly,
    )
    .unwrap();
    let t = crate::collab::identity::create_ticket("collaborator", "t").unwrap();

    assert_eq!(burn_ticket(&t.code, "bob").unwrap(), "collaborator");
    let e = burn_ticket(&t.code, "carol").unwrap_err();
    assert!(e.contains("已被使用"), "err={e}");
}

#[test]
fn federated_user_cannot_be_password_logged_in() {
    let _g = tmp_env();
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
    upsert_member(&c, Admission::ExistingOnly).unwrap();
    // 哨兵不是合法 PHC —— 任何密码都不该进得去。
    assert!(crate::collab::auth::login("alice", "whatever", "d1").is_err());
    assert!(crate::collab::auth::login("alice", FEDERATED_SENTINEL, "d1").is_err());
}


//! 统一账号(邮箱验证码 + 自动 P2P + 权限记忆)的真 HTTP 端到端探针。
//!
//! 单测只验到各模块自己那张表;这个探针补的是**整条链**,而且跨三个独立 collab.db ——
//! 也就是三台互不相识的机器:
//!
//! ```text
//!   云机(账号中心 + 设备目录)      主机 A(我的台式机)     主机 B(我的 NAS)
//!        │  send_code / login_code    │                       │
//!        │◀─ 验证码换断言(首登建号) ──┤                       │
//!        │                        A ── login_assertion ──▶ A 成 owner(空库首登)
//!        │◀─ 同一个人在 B 上登录 ───────────────────────────── B
//!        │                                        B: SelfOwned ✓ **零邀请码**
//! ```
//!
//! 每条断言都对着这次改动要解决的那句抱怨:
//!  1. **发码口存在且报人话** —— 没配 SMTP 时说「没配邮件服务」,不是 500、不是 undefined;
//!  2. **首登即注册** —— 从没注册过的邮箱,验过码就有账号(注册页因此可以消失);
//!  3. **验证码一次性** —— 用过即废,重放拒;
//!  4. **同一个账号的第二台机器零邀请码进得去**(用户那句「不要还得登两个账号」);
//!  5. **别人的账号照旧要邀请码** —— 那条安全铁律一点没松;
//!  6. **本机撤销优先于「我是主人」** —— 丢了电脑在别处点了移出,它就再也进不来;
//!  7. **权限档位落库且记得住** —— 改完重读仍在,冷静期内实际只给只读;
//!  8. **设备台账**含自己那台、含已移出的,并留住首见时刻。
//!
//! 用法:
//!   cargo run -p polaris-collab --example email_login_probe --features collab-host
use serde_json::{json, Value};

fn post(base: &str, path: &str, bearer: Option<&str>, body: Value) -> (u16, Value) {
    let mut req = ureq::post(&format!("{base}{path}"));
    if let Some(t) = bearer {
        req = req.set("Authorization", &format!("Bearer {t}"));
    }
    match req.send_json(body) {
        Ok(r) => (r.status(), r.into_json().unwrap_or(Value::Null)),
        Err(ureq::Error::Status(code, r)) => (code, r.into_json().unwrap_or(Value::Null)),
        Err(e) => panic!("请求 {path} 失败: {e}"),
    }
}

fn get(base: &str, path: &str, bearer: &str) -> (u16, Value) {
    match ureq::get(&format!("{base}{path}"))
        .set("Authorization", &format!("Bearer {bearer}"))
        .call()
    {
        Ok(r) => (r.status(), r.into_json().unwrap_or(Value::Null)),
        Err(ureq::Error::Status(code, r)) => (code, r.into_json().unwrap_or(Value::Null)),
        Err(e) => panic!("请求 {path} 失败: {e}"),
    }
}

fn need(what: &str, ok: bool, detail: impl std::fmt::Debug) {
    if ok {
        println!("  ✓ {what}");
    } else {
        println!("  ✗ {what} —— {detail:?}");
        std::process::exit(1);
    }
}

/// 往某台机器的库里种一枚已知验证码。
///
/// 为什么要探针自己种:真发信要 SMTP 授权码,探针不该依赖外部邮箱服务(也不该往真邮箱发信)。
/// 而「测试模式下固定验证码」那种做法是往生产代码里开一个后门,绝不能做。所以这里直接写库 ——
/// 探针本来就有这台机器的文件系统权限,不绕过任何鉴权逻辑:`login_code` 照样得验哈希。
fn plant_code(db: &std::path::Path, email: &str, code: &str) {
    use argon2::password_hash::{rand_core::OsRng, PasswordHasher, SaltString};
    let salt = SaltString::generate(&mut OsRng);
    let phc = argon2::Argon2::default()
        .hash_password(code.as_bytes(), &salt)
        .unwrap()
        .to_string();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let conn = rusqlite::Connection::open(db).unwrap();
    conn.execute(
        "INSERT INTO email_codes(email,purpose,code_hash,attempts,created_at,expires_at) \
         VALUES(?1,'login',?2,0,?3,?4) \
         ON CONFLICT(email,purpose) DO UPDATE SET code_hash=excluded.code_hash, \
         attempts=0, created_at=excluded.created_at, expires_at=excluded.expires_at",
        rusqlite::params![email, phc, now, now + 600],
    )
    .unwrap();
}

/// 起一台「机器」:独立的 collab.db,可选权威模式。手法与 mesh_probe 相同 ——
/// 三台机器同进程,靠中间件在进路由前把进程级 env 切到各自的库上(探针单线程发请求,不打架)。
async fn spawn_host(dir: &std::path::Path, tag: &str, authority: bool) -> (String, std::path::PathBuf) {
    let db = dir.join(format!("{tag}.db"));
    let key = dir.join(format!("{tag}.key"));
    let db_s = db.to_string_lossy().to_string();
    let key_s = key.to_string_lossy().to_string();
    let is_auth = authority;

    let (tx, _rx) = tokio::sync::broadcast::channel(64);
    let state = polaris_collab::collab::http::CollabState {
        app: polaris_collab::host::AppHandle::new(tx),
        auth_token: std::sync::Arc::new(None),
        advertise: Default::default(),
    };
    let router = polaris_collab::collab::http::collab_router(state, false).layer(
        axum::middleware::from_fn(move |req: axum::extract::Request, next: axum::middleware::Next| {
            let db_s = db_s.clone();
            let key_s = key_s.clone();
            async move {
                std::env::set_var("POLARIS_COLLAB_DB", &db_s);
                std::env::set_var("POLARIS_ACCOUNT_KEY", &key_s);
                if is_auth {
                    std::env::set_var("POLARIS_ACCOUNT_AUTHORITY", "1");
                } else {
                    std::env::remove_var("POLARIS_ACCOUNT_AUTHORITY");
                }
                next.run(req).await
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (base, db)
}

#[tokio::main]
async fn main() {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("polaris-email-login-probe-{stamp}"));
    std::fs::create_dir_all(&dir).unwrap();
    // 官方云机要开自助注册(配合 POLARIS_AUTHORITY_ONLY 把「能跑东西」的一面关掉)。
    std::env::set_var("POLARIS_ACCOUNT_OPEN_SIGNUP", "1");

    let (cloud, cloud_db) = spawn_host(&dir, "cloud", true).await;
    let (host_a, host_a_db) = spawn_host(&dir, "hostA", false).await;
    let (host_b, host_b_db) = spawn_host(&dir, "hostB", false).await;
    println!("云机(账号中心+设备目录) {cloud}");
    println!("主机 A(台式机)          {host_a}");
    println!("主机 B(NAS)             {host_b}\n");

    let out = tokio::task::spawn_blocking(move || {
        probe(Ctx {
            cloud,
            cloud_db,
            host_a,
            host_a_db,
            host_b,
            host_b_db,
        })
    })
    .await;
    if let Err(e) = out {
        eprintln!("探针崩了: {e}");
        std::process::exit(1);
    }
    let _ = std::fs::remove_dir_all(&dir);
    println!("\nALL PASS");
}

struct Ctx {
    cloud: String,
    cloud_db: std::path::PathBuf,
    host_a: String,
    host_a_db: std::path::PathBuf,
    host_b: String,
    host_b_db: std::path::PathBuf,
}

/// 让主机信任云机的公钥(等价于用户在这台机器上第一次登录时 `pin_explicit` 干的事)。
/// 探针直接写 meta,免得为了钉一把公钥去起一整套 TOFU 流程。
///
/// `host` 是这台主机自己的地址 —— 先打它一个无害请求把库建出来。这台机器可能一次请求
/// 都还没接过,collab.db 得等第一次 `open_db()` 才会跑迁移建表。
fn pin_authority(db: &std::path::Path, cloud: &str) {
    let pk = {
        let r = ureq::get(&format!("{cloud}/api/account/pubkey")).call().unwrap();
        let v: Value = r.into_json().unwrap();
        v["publicKey"].as_str().unwrap().to_string()
    };
    // 这台机器可能一次请求都还没接过,库文件都不存在 —— 走 crate 自己的 open_db 跑一次迁移
    // 把表建出来(裸开 sqlite 会得到一个空文件,后面写 meta 就是「no such table」)。
    // 探针是单线程逐条发请求,此刻没有在飞的请求会跟这里抢那个进程级 env。
    std::env::set_var("POLARIS_COLLAB_DB", db);
    let conn = polaris_collab::collab::db::open_db().unwrap();
    for (k, v) in [("authority_url", cloud), ("authority_pub", &pk)] {
        conn.execute(
            "INSERT INTO meta(k,v) VALUES(?1,?2) ON CONFLICT(k) DO UPDATE SET v=excluded.v",
            rusqlite::params![k, v],
        )
        .unwrap();
    }
}

/// 取一张新断言(设备密钥自助换,与桌面后台循环走的是同一条路)。
fn fresh_assertion(cloud: &str, mesh_key: &str) -> String {
    let (code, v) = post(cloud, "/api/mesh/assert", Some(mesh_key), json!({}));
    assert_eq!(code, 200, "换断言失败: {v:?}");
    v["assertion"].as_str().unwrap().to_string()
}

fn probe(c: Ctx) {
    const NODE_A: &str = "node-aaaaaaaaaaaaaaaaaaaa";
    const NODE_B: &str = "node-bbbbbbbbbbbbbbbbbbbb";
    const ME: &str = "wuli@example.com";
    const OTHER: &str = "mallory@example.com";

    // ── ① 发码口:没配 SMTP 时必须报人话,不能是 500 / 空 / undefined ──
    println!("① 发码口");
    let (code, v) = post(&c.cloud, "/api/account/send_code", None, json!({"email": ME}));
    let msg = v["error"].as_str().unwrap_or("");
    need(
        "没配邮件服务时报人话(不是 500/空)",
        code == 400 && msg.contains("邮件服务"),
        (code, &v),
    );

    // ── ② 首登即注册:从没注册过的邮箱,验过码就有账号 ──
    println!("② 首登即注册(注册页可以消失)");
    plant_code(&c.cloud_db, ME, "123456");
    let (code, v) = post(
        &c.cloud,
        "/api/account/login_code",
        None,
        json!({"email": ME, "code": "123456"}),
    );
    need("验证码换到身份断言", code == 200 && v["assertion"].is_string(), &v);
    need("自动建号并签了全局 uid", v["uid"].as_str().is_some_and(|s| s.starts_with("acct_")), &v);
    need(
        "用户名由邮箱前缀派生(用户全程没填过用户名)",
        v["user"]["username"].as_str() == Some("wuli"),
        &v,
    );
    let uid = v["uid"].as_str().unwrap().to_string();
    let assertion_1 = v["assertion"].as_str().unwrap().to_string();

    // ── ③ 验证码一次性:重放必拒 ──
    println!("③ 验证码一次性");
    let (code, v) = post(
        &c.cloud,
        "/api/account/login_code",
        None,
        json!({"email": ME, "code": "123456"}),
    );
    need("用过的码不能再用", code != 200, (code, &v));
    let (code, v) = post(
        &c.cloud,
        "/api/account/login_code",
        None,
        json!({"email": ME, "code": "000000"}),
    );
    need("瞎猜的码进不来", code != 200, (code, &v));

    // ── ④ 两台设备入网(与桌面 account_login_code 的第 ③ 步同一条路)──
    println!("④ 两台设备入网");
    let (code, v) = post(
        &c.cloud,
        "/api/mesh/enroll",
        None,
        json!({"assertion": assertion_1, "nodeId": NODE_A, "name":"台式机","os":"windows","ver":"2.9.0"}),
    );
    need("A 入网拿到设备密钥", code == 200 && v["meshKey"].is_string(), &v);
    let key_a = v["meshKey"].as_str().unwrap().to_string();

    plant_code(&c.cloud_db, ME, "654321");
    let (_, v) = post(
        &c.cloud,
        "/api/account/login_code",
        None,
        json!({"email": ME, "code": "654321"}),
    );
    let assertion_2 = v["assertion"].as_str().unwrap().to_string();
    need("同一个邮箱第二次登录拿到的是同一个 uid", v["uid"].as_str() == Some(&uid), &v);
    let (code, v) = post(
        &c.cloud,
        "/api/mesh/enroll",
        None,
        json!({"assertion": assertion_2, "nodeId": NODE_B, "name":"NAS","os":"linux","ver":"2.9.0"}),
    );
    need("B 入网拿到设备密钥", code == 200 && v["meshKey"].is_string(), &v);
    let key_b = v["meshKey"].as_str().unwrap().to_string();

    // ── ⑤ 设备台账:含自己那台,留住首见时刻 ──
    println!("⑤ 设备台账");
    let (code, v) = get(&c.cloud, "/api/mesh/devices", &key_a);
    let list = v["devices"].as_array().cloned().unwrap_or_default();
    need("台账列出两台", code == 200 && list.len() == 2, &v);
    need(
        "标出了「就是你现在这台」",
        list.iter().any(|d| d["nodeId"] == NODE_A && d["self"] == json!(true)),
        &v,
    );
    need("留住了首见时刻", list.iter().all(|d| d["firstSeen"].as_i64().unwrap_or(0) > 0), &v);

    // 改名:改过之后心跳不许拿机器名覆盖回去。
    let (code, _) = post(
        &c.cloud,
        "/api/mesh/rename",
        Some(&key_a),
        json!({"nodeId": NODE_A, "name": "书房台式机"}),
    );
    need("改名成功", code == 200, code);
    post(
        &c.cloud,
        "/api/mesh/announce",
        Some(&key_a),
        json!({"name":"DESKTOP-1D01IJR","os":"windows","ver":"2.9.0"}),
    );
    let (_, v) = get(&c.cloud, "/api/mesh/devices", &key_a);
    need(
        "心跳没把人取的名字覆盖回机器名",
        v["devices"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["nodeId"] == NODE_A && d["name"] == json!("书房台式机")),
        &v,
    );

    // ── ⑥ 主机 A:空库首登成 owner ──
    println!("⑥ 主机 A:首登成 owner");
    pin_authority(&c.host_a_db, &c.cloud);
    let (code, v) = post(
        &c.host_a,
        "/api/collab/login_assertion",
        None,
        json!({"assertion": fresh_assertion(&c.cloud, &key_a), "deviceId": NODE_A}),
    );
    need("A 上拿到本机会话", code == 200 && v["token"].is_string(), &v);
    need("空库首登即 owner", v["user"]["role"] == json!("owner"), &v);
    let token_a = v["token"].as_str().unwrap().to_string();

    // ── ⑦ 主机 B:**同一个账号的第二台机器,零邀请码** ──
    //     这一条就是用户那句「不要还得登两个账号」在代码里的样子。
    println!("⑦ 主机 B:同账号第二台机器零邀请码");
    pin_authority(&c.host_b_db, &c.cloud);
    let (code, v) = post(
        &c.host_b,
        "/api/collab/login_assertion",
        None,
        json!({"assertion": fresh_assertion(&c.cloud, &key_b), "deviceId": NODE_B}),
    );
    need("B 上也是首登 → owner", code == 200 && v["user"]["role"] == json!("owner"), &v);
    let token_b = v["token"].as_str().unwrap().to_string();
    // B 已经有主人了,此时**同一个人的另一台设备**再进来 —— 走 SelfOwned,仍不要邀请码。
    let (code, v) = post(
        &c.host_b,
        "/api/collab/login_assertion",
        None,
        json!({"assertion": fresh_assertion(&c.cloud, &key_a), "deviceId": NODE_A}),
    );
    need(
        "B 已有主人后,同账号的另一台设备仍然零邀请码进得去",
        code == 200 && v["user"]["role"] == json!("owner"),
        &v,
    );

    // ── ⑧ 安全铁律没松:别人的账号照旧要邀请码 ──
    println!("⑧ 别人的账号照旧要邀请码");
    plant_code(&c.cloud_db, OTHER, "111222");
    let (_, v) = post(
        &c.cloud,
        "/api/account/login_code",
        None,
        json!({"email": OTHER, "code": "111222"}),
    );
    let stranger = v["assertion"].as_str().unwrap().to_string();
    need("陌生人在云端也能注册(云端只管你是谁)", v["uid"].is_string(), &v);
    let (code, v) = post(
        &c.host_b,
        "/api/collab/login_assertion",
        None,
        json!({"assertion": stranger, "deviceId": "node-stranger"}),
    );
    need(
        "但他进不去 B —— 必须要邀请码",
        code != 200 && v["error"].as_str().unwrap_or("").contains("邀请"),
        (code, &v),
    );

    // ── ⑨ 权限档位:落库、记得住、冷静期内实际只给只读 ──
    println!("⑨ 权限档位记得住");
    let (code, v) = get(&c.host_b, "/api/peer/grants", &token_b);
    need("B 上能读到设备信任契约", code == 200, &v);
    let grants = v["grants"].as_array().cloned().unwrap_or_default();
    let a_on_b = grants.iter().find(|g| g["nodeId"] == NODE_A).cloned();
    need("A 已被 B 自动登记(同账号设备)", a_on_b.is_some(), &v);
    let a_on_b = a_on_b.unwrap();
    need(
        "B 上已有主人时新进来的设备进冷静期",
        a_on_b["inCooldown"] == json!(true),
        &a_on_b,
    );
    need(
        "冷静期内:批的是 rw,实际只给 ro",
        a_on_b["fsAccess"] == json!("rw") && a_on_b["effectiveFs"] == json!("ro"),
        &a_on_b,
    );

    // 用户手工调档 → 重读必须还是他调的那个(不能被下一次重连冲回默认值)。
    let (code, _) = post(
        &c.host_b,
        "/api/peer/grant",
        Some(&token_b),
        json!({"nodeId": NODE_A, "fsAccess":"ro", "execAccess":"none", "autoMount": false}),
    );
    need("改档位成功", code == 200, code);
    // 再登一次(等价于重启后重连):档位不该被冲掉。
    post(
        &c.host_b,
        "/api/collab/login_assertion",
        None,
        json!({"assertion": fresh_assertion(&c.cloud, &key_a), "deviceId": NODE_A}),
    );
    let (_, v) = get(&c.host_b, "/api/peer/grants", &token_b);
    let a2 = v["grants"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["nodeId"] == NODE_A)
        .cloned()
        .unwrap();
    need(
        "重连没把用户调过的档冲回默认值",
        a2["fsAccess"] == json!("ro") && a2["execAccess"] == json!("none") && a2["autoMount"] == json!(false),
        &a2,
    );

    // 「信任这台设备」→ 冷静期清零。
    let (code, _) = post(&c.host_b, "/api/peer/trust", Some(&token_b), json!({"nodeId": NODE_A}));
    need("信任按钮生效", code == 200, code);
    let (_, v) = get(&c.host_b, "/api/peer/grants", &token_b);
    let a3 = v["grants"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["nodeId"] == NODE_A)
        .cloned()
        .unwrap();
    need("冷静期已清零", a3["inCooldown"] == json!(false), &a3);

    // ── ⑩ 撤销优先于「我是主人」:丢了电脑,在别处点了移出就再也进不来 ──
    println!("⑩ 撤销优先于「我是主人」");
    let (code, _) = post(&c.host_b, "/api/peer/revoke", Some(&token_b), json!({"nodeId": NODE_A}));
    need("撤销成功", code == 200, code);
    let (code, v) = post(
        &c.host_b,
        "/api/collab/login_assertion",
        None,
        json!({"assertion": fresh_assertion(&c.cloud, &key_a), "deviceId": NODE_A}),
    );
    need(
        "被撤销的设备,连主人本人的账号也进不来",
        code != 200 && v["error"].as_str().unwrap_or("").contains("移出"),
        (code, &v),
    );
    // 别的设备不受牵连。
    let (code, _) = post(
        &c.host_b,
        "/api/collab/login_assertion",
        None,
        json!({"assertion": fresh_assertion(&c.cloud, &key_b), "deviceId": NODE_B}),
    );
    need("其它设备不受牵连", code == 200, code);

    // ── ⑪ 云端移出:名册里立刻消失,设备密钥当场作废 ──
    println!("⑪ 云端移出");
    let (code, _) = post(&c.cloud, "/api/mesh/revoke", Some(&key_b), json!({"nodeId": NODE_A}));
    need("云端移出成功", code == 200, code);
    let (code, v) = post(&c.cloud, "/api/mesh/announce", Some(&key_a), json!({}));
    need(
        "被移出的设备再报到即被拒",
        code != 200 && v["error"].as_str().unwrap_or("").contains("移出"),
        (code, &v),
    );
    let (_, v) = get(&c.cloud, "/api/mesh/devices", &key_b);
    need(
        "台账里仍看得见它(标为已移出)—— 少一台都是缺陷",
        v["devices"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["nodeId"] == NODE_A && d["revoked"] == json!(true)),
        &v,
    );

    // ── ⑫ 成员主机代转发:手机只连得上家里这台机器时,邮箱登录仍然可用 ──
    //     没有这条,「用邮箱登录」在「手机连得上桌面、连不上公网」那种网络下直接不可用,
    //     只能退回粘连接码 —— 那正是这次要消灭的东西。
    println!("⑫ 成员主机代转发(手机连不上公网时)");
    plant_code(&c.cloud_db, ME, "778899");
    let (code, v) = post(
        &c.host_a, // ← 打的是**成员主机**,不是云机
        "/api/account/login_code",
        None,
        json!({"email": ME, "code": "778899"}),
    );
    need(
        "成员主机把验证码转给账号中心并把断言带回来",
        code == 200 && v["assertion"].is_string(),
        (code, &v),
    );
    need("转回来的是同一个 uid", v["uid"].as_str() == Some(&uid), &v);
    // 业务错误也要原样透传,不能糊成「网络错误」让人对着正确的码反复试。
    let (code, v) = post(
        &c.host_a,
        "/api/account/login_code",
        None,
        json!({"email": ME, "code": "000000"}),
    );
    need(
        "错误原文透传(不糊成网络错误)",
        code != 200 && !v["error"].as_str().unwrap_or("").contains("连不上"),
        (code, &v),
    );

    // ── ⑬ 纯账号中心闸:云机不该承载项目(开放注册的前提)──
    println!("⑬ 纯账号中心闸");
    std::env::set_var("POLARIS_AUTHORITY_ONLY", "1");
    let (code, v) = post(
        &c.host_a,
        "/api/collab/projects",
        Some(&token_a),
        json!({"name":"x","repo":"."}),
    );
    need(
        "AUTHORITY_ONLY 下建项目被拒(成员再多也没东西可碰)",
        code == 403 && v["error"].as_str().unwrap_or("").contains("账号中心"),
        (code, &v),
    );
    std::env::remove_var("POLARIS_AUTHORITY_ONLY");
}

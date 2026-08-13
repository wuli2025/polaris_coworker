//! mesh.rs —— 同账号设备网的**客户端**(Tailscale 式「登录即成网」)。
//!
//! 在此之前,接入一台自己的远程主机要手工粘一串 PLRK1 连接码;N 台机器两两互连是 N² 次
//! 粘贴,还要各自保管对端的 owner 令牌。这个模块把那一步整个删掉:
//!
//! ```text
//! 用户在互联页填一次「云端账号中心 + 账号密码」
//!   → 本机拿到身份断言 → 向云端目录入网,换一把长期设备密钥(落 collab.db)
//!   → 后台每 60s:报到 + 取同账号设备清单
//!       对清单里每台还没连上的设备:
//!         iroh 隧道 → 拿新断言去**对端**换它的本机会话 token → fs_mount 挂成本机盘符
//! ```
//!
//! 三条设计上的要紧事:
//!  · **云端从不持有任何设备的访问权。** 它只签「你是 uid X」这句话;进不进得去某台机器,
//!    由那台机器自己的成员资格说了算(`login_assertion` → `upsert_member` 的 ExistingOnly 闸)。
//!    云机被拿下,也开不了你家电脑的盘。
//!  · **设备密钥落本机、可按设备吊销。** 丢一台电脑不必改密码,在别的设备上把它踢出设备网即可。
//!  · **对端离线不拆盘。** 只要它还在设备网名册上,盘符就留着,由 fsmount 看门狗等它回来 ——
//!    拆了再挂会换盘符,正在用的资源管理器窗口全断,那比"暂时读不到"糟得多。
#![cfg(feature = "collab-host")]

use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

/// 心跳周期。权威侧判在线的窗口是它的 3 倍余量(见 collab::mesh::ONLINE_WINDOW)。
const ANNOUNCE_TICK: Duration = Duration::from_secs(60);
/// 首次入网后第一轮对账等多久 —— 别让用户填完账号还要干等一分钟。
const FIRST_TICK: Duration = Duration::from_secs(2);
/// 设备网专用的本地隧道端口起点。手工添加的远程源用 18620+,这里另起一段不打架。
const PORT_BASE: u16 = 18800;
/// 与云端账号中心通信的超时。连不上要快失败:后台循环下一拍再试,不能把线程吊死。
const CLOUD_TIMEOUT: Duration = Duration::from_secs(12);

/// 默认的云端账号中心 —— 官方云机。桌面用户不填地址时用它,自建账号中心的人
/// 照旧在表单里覆盖(前端 `InterconnectView.vue` 的 DEFAULT_AUTHORITY_URL 与此同值)。
const DEFAULT_AUTHORITY: &str = "http://43.139.209.127:8080";

/// 入网地址:用户填了就用用户的;没填先看环境变量(server 形态),最后落到官方云机。
fn default_authority() -> String {
    std::env::var("POLARIS_ACCOUNT_AUTHORITY_URL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_AUTHORITY.to_string())
}

// meta 键(都落在 collab.db,桌面双击启动也能读到 —— 环境变量在这个场景根本不存在)
const K_URL: &str = "mesh_url";
const K_KEY: &str = "mesh_key";
const K_UID: &str = "mesh_uid";

// ────────────────────────────── 本机身份小工具 ──────────────────────────────

fn my_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "我的电脑".into())
}

fn my_os() -> String {
    std::env::consts::OS.to_string()
}

fn my_ver() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// 本机 iroh NodeId。设备网里「一台机器」的身份就是它。
/// 还没就绪(隧道未起/无 collab-net)时返回空串 —— 调用方据此判「稍后重试」。
fn my_node_id() -> String {
    #[cfg(feature = "collab-net")]
    {
        crate::collab::tunnel::host_node_id().unwrap_or_default()
    }
    #[cfg(not(feature = "collab-net"))]
    {
        String::new()
    }
}

// ────────────────────────────── 落库的入网状态 ──────────────────────────────

fn meta(k: &str) -> String {
    crate::collab::db::meta_get(k).unwrap_or_default()
}

struct Cfg {
    url: String,
    key: String,
}

/// 已入网的配置。任一项为空 = 还没入网。
fn cfg() -> Option<Cfg> {
    let url = meta(K_URL);
    let key = meta(K_KEY);
    if url.is_empty() || key.is_empty() {
        return None;
    }
    Some(Cfg { url, key })
}

// ────────────────────────────── 已建立的链路 ──────────────────────────────

struct Link {
    name: String,
    port: u16,
    /// 对端主机签给我的会话 token;None = 还没换到(下一拍重试)。
    token: Option<String>,
    /// 最近一次失败原因(UI 如实展示,不糊成「连接中」)。
    err: String,
}

fn links() -> &'static Mutex<HashMap<String, Link>> {
    static L: OnceLock<Mutex<HashMap<String, Link>>> = OnceLock::new();
    L.get_or_init(|| Mutex::new(HashMap::new()))
}

fn source_id(node_id: &str) -> String {
    format!("mesh-{}", &node_id[..node_id.len().min(16)])
}

/// 给一台新设备分一个本地端口。已在册的沿用原端口(重试不换口,免得旧隧道占着不放)。
fn port_for(node_id: &str) -> u16 {
    let map = links().lock().unwrap();
    if let Some(l) = map.get(node_id) {
        return l.port;
    }
    let used: Vec<u16> = map.values().map(|l| l.port).collect();
    let mut p = PORT_BASE;
    while used.contains(&p) {
        p += 1;
    }
    p
}

// ────────────────────────────── 与云端目录通信 ──────────────────────────────

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(CLOUD_TIMEOUT)
        .timeout_read(CLOUD_TIMEOUT)
        .build()
}

/// 把 ureq 错误翻成人话。云端明确给的业务错误原样透传(「设备已被移出设备网」这类
/// 必须让用户看见原文,糊成「网络错误」会让人对着正确的账号反复试)。
fn cloud_err(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, resp) => {
            let body = resp.into_string().unwrap_or_default();
            serde_json::from_str::<Value>(&body)
                .ok()
                .and_then(|v| v.get("error").and_then(|x| x.as_str()).map(String::from))
                .unwrap_or_else(|| format!("HTTP {code}"))
        }
        ureq::Error::Transport(t) => format!("连不上云端账号中心:{t}"),
    }
}

fn norm_url(u: &str) -> String {
    let t = u.trim().trim_end_matches('/');
    if t.starts_with("http://") || t.starts_with("https://") {
        t.to_string()
    } else {
        format!("http://{t}")
    }
}

/// 心跳 + 取同账号设备清单。
fn announce(c: &Cfg) -> Result<Vec<Value>, String> {
    let resp = agent()
        .post(&format!("{}/api/mesh/announce", c.url))
        .set("Authorization", &format!("Bearer {}", c.key))
        .send_json(json!({ "name": my_name(), "os": my_os(), "ver": my_ver() }))
        .map_err(cloud_err)?;
    let v: Value = resp.into_json().map_err(|e| format!("云端返回的不是 JSON:{e}"))?;
    if let Some(uid) = v.get("uid").and_then(|x| x.as_str()) {
        let _ = crate::collab::db::meta_set(K_UID, uid);
    }
    Ok(v.get("peers")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default())
}

/// 取一张新的身份断言(5 分钟有效),用来进对端主机的门。
fn fresh_assertion(c: &Cfg) -> Result<String, String> {
    let resp = agent()
        .post(&format!("{}/api/mesh/assert", c.url))
        .set("Authorization", &format!("Bearer {}", c.key))
        .send_json(json!({}))
        .map_err(cloud_err)?;
    let v: Value = resp.into_json().map_err(|e| format!("云端返回的不是 JSON:{e}"))?;
    v.get("assertion")
        .and_then(|x| x.as_str())
        .map(String::from)
        .ok_or_else(|| "云端没返回身份断言".into())
}

/// 拿断言去**对端**(经隧道的本地口)换它的本机会话 token。
///
/// 这一步会失败的正常情形:对端还没把我这个账号加成它的成员。此时报错原文就是对端给的
/// 「本主机还没有邀请你」—— 如实展示给用户,他知道该去那台机器上放行。
fn login_peer(port: u16, assertion: &str) -> Result<String, String> {
    let resp = agent()
        .post(&format!("http://127.0.0.1:{port}/api/collab/login_assertion"))
        .send_json(json!({ "assertion": assertion, "deviceId": my_node_id() }))
        .map_err(cloud_err)?;
    let v: Value = resp.into_json().map_err(|e| format!("对端返回的不是 JSON:{e}"))?;
    v.get("token")
        .and_then(|x| x.as_str())
        .map(String::from)
        .ok_or_else(|| "对端没返回会话 token".into())
}

// ────────────────────────────── 对账:清单 → 隧道 + 盘符 ──────────────────────────────

/// 确保一台设备连上并挂成盘。已连上的直接返回(fs_mount 幂等,后续维护交给它的看门狗)。
///
/// 连不连、挂不挂、挂成只读还是可读写,**全看本机的信任契约**(`peer_grants`):
/// 用户在设备台账上调过的档位,这里逐条照办,重启也照办 —— 这就是「记住权限状态」。
async fn ensure_peer(c: &Cfg, node_id: &str, name: &str, uid: &str) {
    let already = links()
        .lock()
        .unwrap()
        .get(node_id)
        .map(|l| l.token.is_some())
        .unwrap_or(false);
    if already {
        return;
    }
    // 首次见到这台同账号设备 → 自动批准(带冷静期);已有契约 = 原样沿用用户调过的档。
    let first = !crate::collab::grants::any_granted().unwrap_or(false);
    let grant = match crate::collab::grants::auto_grant_self(node_id, uid, name, first) {
        Ok((g, _)) => g,
        Err(e) => {
            set_err(node_id, format!("登记设备契约失败:{e}"));
            return;
        }
    };
    if grant.revoked {
        // 用户把这台踢了 —— 名册上还在(云端没踢)但本机不认。别连,也别每拍刷屏。
        return;
    }
    if !grant.should_mount() {
        // 用户把这台的自动挂载关了/盘档设成 none:隧道也不必起,起了只是白占端口。
        return;
    }
    let ro = grant.effective_fs() == crate::collab::grants::FS_RO;
    let drive_hint = grant.drive_hint.clone();
    let port = port_for(node_id);
    // 先登记(带端口),这样重试沿用同一个口。
    links().lock().unwrap().insert(
        node_id.to_string(),
        Link { name: name.to_string(), port, token: None, err: String::new() },
    );

    // 1) 起隧道(幂等:在跑 = no-op)。
    #[cfg(feature = "collab-net")]
    {
        let nid = node_id.to_string();
        let r = tokio::task::spawn_blocking(move || {
            crate::collab::tunnel::connect_client(&nid, port)
        })
        .await;
        if let Ok(Err(e)) = r {
            set_err(node_id, format!("隧道建立失败:{e}"));
            return;
        }
    }
    #[cfg(not(feature = "collab-net"))]
    {
        set_err(node_id, "此构建没有 P2P 隧道(collab-net),设备网只能看不能连".into());
        return;
    }

    // 2) 换对端会话 token。
    let url = c.url.clone();
    let key = c.key.clone();
    let got = tokio::task::spawn_blocking(move || {
        let a = fresh_assertion(&Cfg { url, key })?;
        login_peer(port, &a)
    })
    .await
    .unwrap_or_else(|e| Err(format!("登录任务失败:{e}")));
    let token = match got {
        Ok(t) => t,
        Err(e) => {
            set_err(node_id, e);
            return;
        }
    };

    // 3) 挂盘。fs_mount 幂等且自带看门狗:对端未就绪也只是"稍后自动挂上"。
    //    ro=true 时钉死只读 —— 冷静期内的新设备走的就是这条(对端就算开着写位也不给写)。
    //    drive_hint 是上次的盘符,尽量复原:每次重启换个字母,用户存的快捷方式和脚本全断。
    let sid = source_id(node_id);
    match crate::fsmount::fs_mount(
        sid,
        name.to_string(),
        node_id.to_string(),
        port,
        token.clone(),
        Some(ro),
        Some(drive_hint),
    )
    .await
    {
        Ok(v) => {
            let mut map = links().lock().unwrap();
            if let Some(l) = map.get_mut(node_id) {
                l.token = Some(token);
                l.err.clear();
            }
            drop(map);
            // 记住这次挂成了哪个盘符,下次优先复原它。
            if let Some(d) = v["drive"].as_str().filter(|s| !s.is_empty()) {
                let _ = crate::collab::grants::remember_drive(node_id, d);
            }
            eprintln!(
                "[mesh] 「{name}」已接入(端口 {port},盘:{}{})",
                v["drive"],
                if ro { ",只读" } else { "" }
            );
        }
        Err(e) => set_err(node_id, format!("挂盘失败:{e}")),
    }
}

fn set_err(node_id: &str, e: String) {
    eprintln!("[mesh] {node_id} 接入未成:{e}");
    let mut map = links().lock().unwrap();
    if let Some(l) = map.get_mut(node_id) {
        l.err = e;
    }
}

/// 从设备网名册里消失的设备(被吊销/被踢):拆盘 + 断隧道。
/// **只在"名册里没了"时拆** —— 单纯离线不拆(见文件头第三条)。
async fn drop_peer(node_id: &str) {
    let l = links().lock().unwrap().remove(node_id);
    let Some(l) = l else { return };
    let _ = crate::fsmount::fs_unmount(source_id(node_id)).await;
    #[cfg(feature = "collab-net")]
    {
        let port = l.port;
        let _ = tokio::task::spawn_blocking(move || {
            crate::collab::tunnel::disconnect_client(port)
        })
        .await;
    }
    eprintln!("[mesh] 「{}」已移出设备网,盘符已收回", l.name);
}

/// 一轮对账。返回 (清单条数, 错误) —— 错误只用于日志/状态,不中断循环。
async fn reconcile_once() -> Result<usize, String> {
    let Some(c) = cfg() else {
        return Err("本机还没入网".into());
    };
    let c2 = Cfg { url: c.url.clone(), key: c.key.clone() };
    let peers = tokio::task::spawn_blocking(move || announce(&c2))
        .await
        .unwrap_or_else(|e| Err(format!("报到任务失败:{e}")))?;

    let uid = meta(K_UID);
    let mut seen: Vec<String> = Vec::new();
    for p in &peers {
        let node_id = p.get("node_id").and_then(|x| x.as_str()).unwrap_or("");
        if node_id.is_empty() {
            continue;
        }
        let name = p
            .get("name")
            .and_then(|x| x.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("同账号设备");
        seen.push(node_id.to_string());
        ensure_peer(&c, node_id, name, &uid).await;
    }
    // 本机撤销的设备:名册上可能还在(云端没踢),但本机不认 —— 立刻收回它的盘和隧道。
    // 「在别的设备上点了移出」要**当场**生效,不能等到下次重启。
    let revoked_now: Vec<String> = links()
        .lock()
        .unwrap()
        .keys()
        .filter(|k| crate::collab::grants::is_revoked(k))
        .cloned()
        .collect();
    for r in revoked_now {
        eprintln!("[mesh] 「{r}」已被本机撤销,收回盘符");
        drop_peer(&r).await;
    }
    // 名册里没了的,收回。
    let gone: Vec<String> = links()
        .lock()
        .unwrap()
        .keys()
        .filter(|k| !seen.contains(k))
        .cloned()
        .collect();
    for g in gone {
        drop_peer(&g).await;
    }
    Ok(peers.len())
}

// ────────────────────────────── 后台循环 ──────────────────────────────

fn rt() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("mesh")
            .enable_all()
            .build()
            .expect("mesh runtime")
    })
}

static RUNNING: AtomicBool = AtomicBool::new(false);

/// 起后台对账循环。幂等:已在跑 = no-op(重复调 mesh_join 不会起出两个循环)。
fn spawn_loop() {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    rt().spawn(async {
        tokio::time::sleep(FIRST_TICK).await;
        loop {
            if cfg().is_none() {
                // 已退网:收干净所有链路后停循环。
                let all: Vec<String> = links().lock().unwrap().keys().cloned().collect();
                for n in all {
                    drop_peer(&n).await;
                }
                RUNNING.store(false, Ordering::SeqCst);
                return;
            }
            match reconcile_once().await {
                Ok(n) => eprintln!("[mesh] 对账完成:名册 {n} 台"),
                Err(e) => eprintln!("[mesh] 对账失败(下一拍重试):{e}"),
            }
            tokio::time::sleep(ANNOUNCE_TICK).await;
        }
    });
}

/// 开机时如果已入网就自动接上。桌面启动流程里调一次即可 —— 用户填过一次账号之后,
/// 此后每次开机都是「打开就已经连好」。
pub fn spawn_if_enrolled() {
    if cfg().is_some() {
        eprintln!("[mesh] 已入网,后台自动接入同账号设备");
        spawn_loop();
    }
}

// ────────────────────────────── 命令 ──────────────────────────────

/// 发一枚登录验证码到邮箱。整套登录流程的第一步,也是用户唯一要动手的两步之一。
///
/// `url` 留空 = 官方云端账号中心。这个参数只为自建账号中心的人保留,UI 收在「高级」里。
#[cfg_attr(feature = "desktop", tauri::command)]
#[allow(non_snake_case)]
pub async fn account_send_code(email: String, url: Option<String>) -> Result<Value, String> {
    let url = norm_url(&authority_of(url));
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        agent()
            .post(&format!("{url}/api/account/send_code"))
            .send_json(json!({ "email": email.trim() }))
            .map_err(cloud_err)?;
        Ok(json!({ "sent": true }))
    })
    .await
    .map_err(|e| format!("发码任务失败:{e}"))??;
    Ok(out)
}

/// **一步登录**:邮箱 + 验证码 → 从此这台机器就在你的设备网里。
///
/// 用户只做两件事(填邮箱、填码),下面这四件事全在这一个命令里做完 ——
/// 此前它们分散在「注册页 / 登录页 / 设为主机 / 入网表单」四个地方,那正是「怎么还要登两个账号」
/// 的来源:
///   ① 拿验证码换身份断言(账号不存在就当场建号 —— 注册页因此可以整个消失);
///   ② 把这个账号中心**钉在本机**(少了它,别的设备连过来会被自己挡回去);
///   ③ 拿断言入网,换一把长期设备密钥落库(此后再不需要邮箱验证码);
///   ④ 拿断言进**本机**的门,拿到本机会话 —— 同 uid 走 SelfOwned,不要邀请码。
///
/// 任何一步失败都给人话,并且**已经做成的步骤不回滚**:比如网络在第 ④ 步断了,
/// ①②③ 的成果(设备密钥)留着,下次开机后台自己就接上了,不必让用户从头再来一遍。
#[cfg_attr(feature = "desktop", tauri::command)]
#[allow(non_snake_case)]
pub async fn account_login_code(
    email: String,
    code: String,
    url: Option<String>,
) -> Result<Value, String> {
    let url = norm_url(&authority_of(url));
    // NodeId 由本机密钥推导,不需要隧道在跑 —— 老版本那句「请先设为主机」其实是误导:
    // 真正需要主机在跑的是「别人连得进来」,不是「我拿得到身份」。所以这里不再拦人,
    // 拿不到才报错(那是密钥文件写不进去,是真问题)。
    let node_id = my_node_id();
    if node_id.is_empty() {
        return Err("本机 P2P 身份生成失败(数据目录写不进去?)—— 换个数据目录再试".into());
    }
    let out = tokio::task::spawn_blocking({
        let url = url.clone();
        let node_id = node_id.clone();
        move || -> Result<Value, String> {
            // ① 验证码 → 身份断言(账号不存在则当场建号,由账号中心决定)。
            let resp = agent()
                .post(&format!("{url}/api/account/login_code"))
                .send_json(json!({ "email": email.trim(), "code": code.trim() }))
                .map_err(cloud_err)?;
            let v: Value = resp
                .into_json()
                .map_err(|e| format!("账号中心返回的不是 JSON:{e}"))?;
            let assertion = v
                .get("assertion")
                .and_then(|x| x.as_str())
                .ok_or("账号中心没返回身份断言 —— 确认这个地址开了 POLARIS_ACCOUNT_AUTHORITY=1")?
                .to_string();
            let email_back = v
                .get("user")
                .and_then(|u| u.get("email"))
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let mut out = enroll_with_assertion(&url, &node_id, &assertion)?;
            out["email"] = json!(email_back);
            Ok(out)
        }
    })
    .await
    .map_err(|e| format!("登录任务失败:{e}"))??;

    finish_enroll(&url, &node_id, &out)
}

/// 已有一张断言时的入网 + 进本机门。`account_login_code` 与 `mesh_join` 共用这一段,
/// 免得两条路各写一份、慢慢漂成两个语义。**阻塞**,调用方须放 spawn_blocking。
fn enroll_with_assertion(url: &str, node_id: &str, assertion: &str) -> Result<Value, String> {
    // ③ 拿断言入网,换一把长期设备密钥。
    let resp = agent()
        .post(&format!("{url}/api/mesh/enroll"))
        .send_json(json!({
            "assertion": assertion,
            "nodeId": node_id,
            "name": my_name(),
            "os": my_os(),
            "ver": my_ver(),
        }))
        .map_err(cloud_err)?;
    let v: Value = resp
        .into_json()
        .map_err(|e| format!("账号中心返回的不是 JSON:{e}"))?;
    let key = v
        .get("meshKey")
        .and_then(|x| x.as_str())
        .ok_or("账号中心没返回设备密钥")?
        .to_string();
    let uid = v.get("uid").and_then(|x| x.as_str()).unwrap_or("").to_string();

    // ② 把这个账号中心**钉在本机**。少了这一步,设备网只能单向:本机连得出去,
    //    别的设备连过来会被自己挡回去(「本机未配置云端账号中心,不接受身份断言」)。
    //    桌面是双击启动的,POLARIS_ACCOUNT_AUTHORITY_URL 这种环境变量在这儿不存在,
    //    所以用户亲手登录的这一刻,就是钉住它唯一合法的时机。
    let pin = crate::collab::authority::pin_explicit(url);
    // ④ 拿断言进本机的门。同 uid 走 SelfOwned —— 不要邀请码,这就是「不再登两次」。
    //    本机已被别人占着(同名本地账号等)时会失败,那时只是「别人进不来」,不影响连出去。
    let member = crate::collab::authority::login_with_assertion(assertion, node_id)
        .map(|(u, t)| (u.role, t));
    Ok(json!({
        "key": key,
        "uid": uid,
        "pinWarn": pin.err().unwrap_or_default(),
        "memberRole": member.as_ref().map(|(r, _)| r.clone()).unwrap_or_default(),
        // 本机会话 token:前端拿它直接调本机的 /api/peer/* 管设备档位,
        // 不必让用户在自己的机器上再登一次。
        "token": member.as_ref().map(|(_, t)| t.clone()).unwrap_or_default(),
        "memberWarn": member.err().unwrap_or_default(),
    }))
}

/// 入网成功后的落库 + 起循环。同步小操作,单独抽出来是为了让两条登录路共用同一份收尾。
fn finish_enroll(url: &str, node_id: &str, out: &Value) -> Result<Value, String> {
    crate::collab::db::meta_set(K_URL, url)?;
    crate::collab::db::meta_set(K_KEY, out["key"].as_str().unwrap_or(""))?;
    crate::collab::db::meta_set(K_UID, out["uid"].as_str().unwrap_or(""))?;
    spawn_loop();
    Ok(json!({
        "enrolled": true,
        "url": url,
        "uid": out["uid"],
        "email": out.get("email").cloned().unwrap_or(json!("")),
        "nodeId": node_id,
        "token": out["token"],
        // 两条「半成品」警告如实上报,UI 据此告诉用户「还差一步」而不是假装全好了:
        //  · pinWarn   :本机之前信任着**另一个**账号中心 → 别的设备连不进来;
        //  · memberWarn:这个账号还没成为本机成员(比如本机已有同名本地账号)→ 同上。
        "pinWarn": out["pinWarn"],
        "memberWarn": out["memberWarn"],
        "memberRole": out["memberRole"],
    }))
}

/// 入网地址:调用方填了就用它,没填看环境变量(server 形态),最后落到官方云机。
fn authority_of(url: Option<String>) -> String {
    url.map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(default_authority)
}

/// 拿一张**已有的**断言入网(手机壳/脚本等已经自己登过的场景)。
/// 桌面 UI 走的是 [`account_login_code`],不经过这里。
#[cfg_attr(feature = "desktop", tauri::command)]
#[allow(non_snake_case)]
pub async fn mesh_join(assertion: String, url: Option<String>) -> Result<Value, String> {
    let node_id = my_node_id();
    if node_id.is_empty() {
        return Err("本机 P2P 身份生成失败(数据目录写不进去?)—— 换个数据目录再试".into());
    }
    let url = norm_url(&authority_of(url));
    let out = tokio::task::spawn_blocking({
        let url = url.clone();
        let node_id = node_id.clone();
        move || enroll_with_assertion(&url, &node_id, &assertion)
    })
    .await
    .map_err(|e| format!("入网任务失败:{e}"))??;
    finish_enroll(&url, &node_id, &out)
}

/// **设备台账**:一次给全 —— 云端目录(有哪些设备)+ 本机契约(各自什么档)+ 实时链路
/// (连上没、挂成哪个盘)。前端一个请求画完整页,不必自己 join 三份数据。
///
/// 云端拉不到(断网/云机挂了)不算失败:退回只用本机契约与实时链路画,并把原因带上 ——
/// 断网时看见「3 台设备,云端暂时联系不上」远好过看见一片空白。
#[cfg_attr(feature = "desktop", tauri::command)]
pub async fn mesh_devices() -> Result<Value, String> {
    let me = my_node_id();
    let (cloud, cloud_msg) = match cfg() {
        Some(c) => {
            let r = tokio::task::spawn_blocking(move || -> Result<Value, String> {
                let resp = agent()
                    .get(&format!("{}/api/mesh/devices", c.url))
                    .set("Authorization", &format!("Bearer {}", c.key))
                    .call()
                    .map_err(cloud_err)?;
                resp.into_json::<Value>()
                    .map_err(|e| format!("云端返回的不是 JSON:{e}"))
            })
            .await
            .unwrap_or_else(|e| Err(format!("取台账任务失败:{e}")));
            match r {
                Ok(v) => (v, String::new()),
                Err(e) => (json!({}), e),
            }
        }
        None => (json!({}), "本机还没登录".to_string()),
    };

    // 本机契约:node_id → 档位。
    let grants: HashMap<String, crate::collab::grants::Grant> =
        crate::collab::grants::list()
            .unwrap_or_default()
            .into_iter()
            .map(|g| (g.node_id.clone(), g))
            .collect();
    // 实时链路 + 盘。
    let mounts: HashMap<String, Value> = crate::fsmount::fs_mount_status()
        .into_iter()
        .filter_map(|v| {
            v.get("sourceId")
                .and_then(|x| x.as_str())
                .map(|s| (s.to_string(), v.clone()))
        })
        .collect();
    let links_snapshot: HashMap<String, (bool, String)> = links()
        .lock()
        .unwrap()
        .iter()
        .map(|(k, l)| (k.clone(), (l.token.is_some(), l.err.clone())))
        .collect();

    // 云端台账为骨架;云端拉不到就拿本机契约当骨架(断网也要能看见自己批过哪些设备)。
    let skeleton: Vec<Value> = match cloud.get("devices").and_then(|x| x.as_array()) {
        Some(list) => list.clone(),
        None => grants
            .values()
            .map(|g| {
                json!({
                    "nodeId": g.node_id, "name": g.name, "os": "", "ver": "",
                    "lastSeen": 0, "online": false, "firstSeen": g.granted_at,
                    "revoked": g.revoked, "self": g.node_id == me,
                })
            })
            .collect(),
    };

    let devices: Vec<Value> = skeleton
        .into_iter()
        .map(|mut d| {
            let nid = d.get("nodeId").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let g = grants.get(&nid);
            let m = mounts.get(&source_id(&nid));
            let (connected, err) = links_snapshot
                .get(&nid)
                .cloned()
                .unwrap_or((false, String::new()));
            d["isSelf"] = json!(d.get("self").and_then(|x| x.as_bool()).unwrap_or(nid == me));
            d["connected"] = json!(connected);
            d["error"] = json!(err);
            d["drive"] = m.and_then(|x| x.get("drive")).cloned().unwrap_or(json!(""));
            d["mounted"] = m.and_then(|x| x.get("ok")).cloned().unwrap_or(json!(false));
            d["writable"] = m.and_then(|x| x.get("writable")).cloned().unwrap_or(json!(false));
            // 档位:没有契约的设备(还没连过)给个「未批准」而不是编一个默认值。
            d["fsAccess"] = json!(g.map(|x| x.fs_access.clone()).unwrap_or_default());
            d["effectiveFs"] = json!(g.map(|x| x.effective_fs().to_string()).unwrap_or_default());
            d["execAccess"] = json!(g.map(|x| x.exec_access.clone()).unwrap_or_default());
            d["autoMount"] = json!(g.map(|x| x.auto_mount).unwrap_or(false));
            d["inCooldown"] = json!(g.map(|x| x.in_cooldown()).unwrap_or(false));
            d["cooldownUntil"] = json!(g.map(|x| x.cooldown_until).unwrap_or(0));
            d["granted"] = json!(g.is_some());
            d["localRevoked"] = json!(g.map(|x| x.revoked).unwrap_or(false));
            d
        })
        .collect();

    Ok(json!({
        "enrolled": cfg().is_some(),
        "url": meta(K_URL),
        "uid": meta(K_UID),
        "nodeId": me,
        "name": my_name(),
        "cloudError": cloud_msg,
        "devices": devices,
    }))
}

/// 改一台设备在**本机**的档位(盘 none/ro/rw、执行 none/ask/allow、自动挂载)。
///
/// 立刻落库(重启仍在),并当场生效:关掉自动挂载/降成 none 会把已挂的盘收回来,
/// 改回来则下一拍对账自动挂上 —— 不必让用户重启应用才看见效果。
#[cfg_attr(feature = "desktop", tauri::command)]
#[allow(non_snake_case)]
pub async fn peer_grant_set(
    nodeId: String,
    fsAccess: Option<String>,
    execAccess: Option<String>,
    autoMount: Option<bool>,
) -> Result<Value, String> {
    let g = crate::collab::grants::update(
        &nodeId,
        None,
        fsAccess.as_deref(),
        execAccess.as_deref(),
        autoMount,
        "本机",
    )?;
    if !g.should_mount() {
        drop_peer(&nodeId).await;
    } else {
        // 档位变了(比如 ro→rw)要重挂才生效:writable 是挂载那一刻钉进系统的。
        drop_peer(&nodeId).await;
        let _ = mesh_sync().await;
    }
    Ok(json!({ "ok": true, "effectiveFs": g.effective_fs(), "autoMount": g.auto_mount }))
}

/// 「信任这台设备」:冷静期清零,全权立即生效(顺带把误撤销的恢复回来)。
#[cfg_attr(feature = "desktop", tauri::command)]
#[allow(non_snake_case)]
pub async fn peer_trust(nodeId: String) -> Result<Value, String> {
    let g = crate::collab::grants::trust_now(&nodeId, "本机")?;
    // 冷静期里那块盘是钉死只读挂上的,清零后必须重挂才真的可写。
    drop_peer(&nodeId).await;
    let _ = mesh_sync().await;
    Ok(json!({ "ok": true, "effectiveFs": g.effective_fs() }))
}

/// 撤销一台设备在**本机**的准入,并当场收回它的盘与隧道。
///
/// 与云端的 [`mesh_kick`] 是两道独立的闸,丢了电脑应该都点:
/// 云端那道让它拿不到新断言,本机这道让它连断言都递不进来。
#[cfg_attr(feature = "desktop", tauri::command)]
#[allow(non_snake_case)]
pub async fn peer_revoke(nodeId: String) -> Result<Value, String> {
    crate::collab::grants::revoke(&nodeId, "本机")?;
    drop_peer(&nodeId).await;
    Ok(json!({ "ok": true }))
}

/// 给设备改个人话名字(改的是云端台账,所有设备上都跟着变)。
#[cfg_attr(feature = "desktop", tauri::command)]
#[allow(non_snake_case)]
pub async fn mesh_rename(nodeId: String, name: String) -> Result<Value, String> {
    let Some(c) = cfg() else {
        return Err("本机还没登录".into());
    };
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        agent()
            .post(&format!("{}/api/mesh/rename", c.url))
            .set("Authorization", &format!("Bearer {}", c.key))
            .send_json(json!({ "nodeId": nodeId, "name": name }))
            .map_err(cloud_err)?;
        Ok(())
    })
    .await
    .map_err(|e| format!("改名任务失败:{e}"))??;
    Ok(json!({ "ok": true }))
}

/// 退网:清掉本机的设备密钥,拆掉所有自动挂上的盘。
/// 云端名册上这台设备仍在(只是不再报到,很快显示离线)—— 要彻底踢掉用 [`mesh_kick`]。
#[cfg_attr(feature = "desktop", tauri::command)]
pub async fn mesh_leave() -> Result<Value, String> {
    let _ = crate::collab::db::meta_set(K_KEY, "");
    let all: Vec<String> = links().lock().unwrap().keys().cloned().collect();
    for n in all {
        drop_peer(&n).await;
    }
    Ok(json!({ "enrolled": false }))
}

/// 把自己账号名下的某台设备移出设备网(丢了电脑的应急操作)。
#[cfg_attr(feature = "desktop", tauri::command)]
#[allow(non_snake_case)]
pub async fn mesh_kick(nodeId: String) -> Result<Value, String> {
    let Some(c) = cfg() else {
        return Err("本机还没入网".into());
    };
    let nid = nodeId.clone();
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        agent()
            .post(&format!("{}/api/mesh/revoke", c.url))
            .set("Authorization", &format!("Bearer {}", c.key))
            .send_json(json!({ "nodeId": nid }))
            .map_err(cloud_err)?;
        Ok(())
    })
    .await
    .map_err(|e| format!("吊销任务失败:{e}"))??;
    drop_peer(&nodeId).await;
    Ok(json!({ "ok": true }))
}

/// 立刻对一次账(UI 点「刷新」用,不必等下一拍心跳)。
#[cfg_attr(feature = "desktop", tauri::command)]
pub async fn mesh_sync() -> Result<Value, String> {
    let n = reconcile_once().await?;
    Ok(json!({ "peers": n }))
}

/// 设备网现状:入网了吗、名册上有谁、各自连上了没、挂成了哪块盘。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn mesh_status() -> Value {
    let enrolled = cfg().is_some();
    // fsmount 的实况(盘符/读写档/在线)按 sourceId 对进来,前端一处就能全画出来。
    let mounts: HashMap<String, Value> = crate::fsmount::fs_mount_status()
        .into_iter()
        .filter_map(|v| {
            v.get("sourceId")
                .and_then(|x| x.as_str())
                .map(|s| (s.to_string(), v.clone()))
        })
        .collect();
    let peers: Vec<Value> = links()
        .lock()
        .unwrap()
        .iter()
        .map(|(node_id, l)| {
            let m = mounts.get(&source_id(node_id));
            json!({
                "nodeId": node_id,
                "name": l.name,
                "port": l.port,
                "connected": l.token.is_some(),
                "error": l.err,
                // 对端会话 token 交给前端:远程终端(/api/exec)与「浏览盘」都靠它。
                // 与手工添加的远程源同一口径 —— 那条路的 owner 令牌本来就存在前端。
                "token": l.token.clone().unwrap_or_default(),
                "drive": m.and_then(|x| x.get("drive")).cloned().unwrap_or(json!("")),
                "writable": m.and_then(|x| x.get("writable")).cloned().unwrap_or(json!(false)),
                "ok": m.and_then(|x| x.get("ok")).cloned().unwrap_or(json!(false)),
            })
        })
        .collect();
    json!({
        "enrolled": enrolled,
        "url": meta(K_URL),
        "uid": meta(K_UID),
        "nodeId": my_node_id(),
        "name": my_name(),
        "peers": peers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_normalization() {
        assert_eq!(norm_url(" 1.2.3.4:8080/ "), "http://1.2.3.4:8080");
        assert_eq!(norm_url("https://a.com/"), "https://a.com");
        assert_eq!(norm_url("http://a.com"), "http://a.com");
    }

    /// sourceId 必须稳定且不超长(fsmount 拿它当 key,长 NodeId 截断后仍要唯一到实用)。
    #[test]
    fn source_id_is_stable_and_short() {
        let a = "abcdefghijklmnopqrstuvwxyz0123456789";
        assert_eq!(source_id(a), source_id(a));
        assert_eq!(source_id(a), "mesh-abcdefghijklmnop");
        assert_eq!(source_id("short"), "mesh-short");
    }

    /// 端口分配:新设备各得一口,已在册的沿用原口(重试不换口)。
    #[test]
    fn ports_are_unique_and_sticky() {
        links().lock().unwrap().clear();
        let p1 = port_for("node-a");
        links().lock().unwrap().insert(
            "node-a".into(),
            Link { name: "a".into(), port: p1, token: None, err: String::new() },
        );
        let p2 = port_for("node-b");
        assert_ne!(p1, p2, "两台设备不能撞同一个本地口");
        assert_eq!(port_for("node-a"), p1, "已在册的必须沿用原端口");
        links().lock().unwrap().clear();
    }
}

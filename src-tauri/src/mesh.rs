//! mesh.rs —— 同账号设备网的**客户端**(Tailscale 式「登录即成网」)。
//!
//! 在此之前,接入一台自己的远程主机要手工粘一串 PLRK1 连接码;N 台机器两两互连是 N² 次
//! 粘贴,还要各自保管对端的 owner 令牌。这个模块把那一步整个删掉:
//!
//! ```text
//! 用户在统一登录页填「邮箱 + 验证码」
//!   → 本机拿到身份断言 → pin 账号中心并通过本机成员准入
//!   → 向云端目录入网,把长期设备密钥 + UID + URL 原子落入 collab.db
//!   → 后台每 60s:报到 + 取同账号设备清单
//!       对清单里的设备实探 tunnel/session:
//!         iroh 隧道 → 拿新断言去**对端**换它的本机会话 token → fs_mount 挂成本机盘符
//! ```
//!
//! 三条设计上的要紧事:
//!  · **云端不持有远程盘会话或设备私钥。** 它只签身份断言；对端还要经过自己的成员资格、
//!    设备 proof、grant/冷静期与撤销闸。账号中心签名权一旦失陷仍是严重事件（可伪造身份），
//!    因而公网传输必须 HTTPS、authority 必须 pin，不能把“云端不存盘 token”夸成绝对安全。
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

/// Placeholder bundle means local mode; configuration errors remain visible in status.
fn default_authority() -> String {
    match crate::collab::interconnect::load() {
        Ok(config) => config.authority.url.map_or_else(String::new, |url| url),
        Err(_) => String::new(),
    }
}

mod connections;
use connections::ensure_peer;
pub use connections::*;
mod security;
pub use security::*;
mod login;
mod orchestration;
pub use login::*;

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

/// 账号 UID 只有在设备密钥仍有效时才是当前身份。旧版本退网只清 `mesh_key`，因此这里也
/// fail closed：哪怕磁盘里残留过 UID，未入网状态也绝不再把它当成当前账号回给 UI。
fn current_uid(enrolled: bool, stored: String) -> String {
    if enrolled {
        stored
    } else {
        String::new()
    }
}

fn visible_uid() -> String {
    current_uid(cfg().is_some(), meta(K_UID))
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
    /// 当前链路阶段：connecting / authenticating / mounting / ready / degraded。
    phase: String,
    /// 稳定错误码，UI 不再靠中文文案猜故障类型。
    error_code: String,
    /// 最近一次 tunnel + token 探测同时通过的 unix 秒。
    last_verified_at: u64,
    /// 最近一次失败原因(UI 如实展示,不糊成「连接中」)。
    err: String,
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn link_connected(link: &Link) -> bool {
    let recently_verified = link.token.is_some()
        && link.last_verified_at > 0
        && now_secs().saturating_sub(link.last_verified_at) <= 180;
    #[cfg(feature = "collab-net")]
    {
        recently_verified && crate::collab::tunnel::client_state(link.port).connected
    }
    #[cfg(not(feature = "collab-net"))]
    {
        let _ = recently_verified;
        false
    }
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
        format!("https://{t}")
    }
}

fn loopback_http(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("http://") else {
        return false;
    };
    let authority = rest.split('/').next().unwrap_or("");
    let host = authority.rsplit('@').next().unwrap_or(authority);
    host == "localhost"
        || host.starts_with("localhost:")
        || host == "127.0.0.1"
        || host.starts_with("127.0.0.1:")
        || host == "[::1]"
        || host.starts_with("[::1]:")
}

/// 登录凭据、OTP、assertion 与长期 meshKey 都会经过账号中心。公网 HTTP 必须显式确认，
/// 不能再因为用户把“高级地址”留空，就静默把整套身份发到明文 IP。
fn validate_authority_url(raw: &str, allow_insecure_http: bool) -> Result<String, String> {
    if raw.trim().is_empty() {
        return Err("未配置权威；当前可继续使用本地模式".into());
    }
    let url = norm_url(raw);
    if url == "https://" || url == "http://" {
        return Err("请填写账号中心地址".into());
    }
    if url.starts_with("https://") || loopback_http(&url) {
        return Ok(url);
    }
    if url.starts_with("http://") {
        if allow_insecure_http {
            return Ok(url);
        }
        return Err(format!(
            "账号中心 {url} 使用明文 HTTP，邮箱、验证码和设备密钥可能被窃取。请改用 HTTPS；若这是你明确信任的旧服务，请在高级设置勾选‘仍使用不安全 HTTP’后重试"
        ));
    }
    Err("账号中心地址只支持 https://（本机调试可用 http://localhost）".into())
}

fn authority_transport_state(url: &str) -> &'static str {
    if url.starts_with("https://") || loopback_http(url) {
        "secure"
    } else {
        "insecure_legacy"
    }
}

/// 心跳 + 取同账号设备清单。
fn announce(c: &Cfg) -> Result<Value, String> {
    crate::collab::authority::pin_explicit(&c.url)?;
    crate::collab::db::meta_set("mesh_node_id", &my_node_id())?;
    let resp = agent()
        .post(&format!("{}/api/mesh/announce", c.url))
        .set("Authorization", &format!("Bearer {}", c.key))
        .send_json(json!({ "name": my_name(), "os": my_os(), "ver": my_ver() }))
        .map_err(cloud_err)?;
    let v: Value = resp
        .into_json()
        .map_err(|e| format!("云端返回的不是 JSON:{e}"))?;
    if let Some(uid) = v.get("uid").and_then(|x| x.as_str()) {
        let _ = crate::collab::db::meta_set(K_UID, uid);
    }
    security::publish(&v);
    Ok(v)
}

/// 取一张新的身份断言(5 分钟有效),用来进对端主机的门。
fn fresh_assertion(c: &Cfg) -> Result<String, String> {
    crate::collab::authority::pin_explicit(&c.url)?;
    let resp = agent()
        .post(&format!("{}/api/mesh/assert", c.url))
        .set("Authorization", &format!("Bearer {}", c.key))
        .send_json(json!({}))
        .map_err(cloud_err)?;
    let v: Value = resp
        .into_json()
        .map_err(|e| format!("云端返回的不是 JSON:{e}"))?;
    v.get("assertion")
        .and_then(|x| x.as_str())
        .map(String::from)
        .ok_or_else(|| "云端没返回身份断言".into())
}

/// 拿断言去**对端**(经隧道的本地口)换它的本机会话 token。
///
/// 这一步会失败的正常情形:对端还没把我这个账号加成它的成员。此时报错原文就是对端给的
/// 「本主机还没有邀请你」—— 如实展示给用户,他知道该去那台机器上放行。
/// 从设备网名册里消失的设备(被吊销/被踢):拆盘 + 断隧道。
/// **只在"名册里没了"时拆** —— 单纯离线不拆(见文件头第三条)。
async fn drop_peer(node_id: &str) {
    let l = links().lock().unwrap().remove(node_id);
    // 挂载注册表和链路表应当同生共死，但撤权路径必须以“收回能力”为准：即使某次
    // 异常只留下了挂载，也照样幂等卸掉，不能因链路记录缺失而提前返回。
    let _ = crate::fsmount::unmount_authorized(source_id(node_id)).await;
    let Some(l) = l else { return };
    #[cfg(feature = "collab-net")]
    {
        let port = l.port;
        let _ = tokio::task::spawn_blocking(move || crate::collab::tunnel::disconnect_client(port))
            .await;
    }
    eprintln!("[mesh] 「{}」已移出设备网,盘符已收回", l.name);
}

/// 本机成员资格的**旧状态自愈**：历史版本可能已先保存 meshKey，却没能进自己这台机器的门。
///
/// 新登录现在会在云端 enroll 前完成本机准入，不再产生这种半成品；这里仍要照顾磁盘上已经
/// 存在的旧状态，否则升级后登录卡不会再出现，而设备之间仍会因为缺少本机成员资格互相挡回去。
///
/// 便宜：先纯本地读一次成员资格（命中即 no-op，这是绝大多数情况），真的缺才去取一张断言。
/// 用的是本机自己的设备密钥 + [`login_with_assertion_local`]，与用户在这台机器前亲手登录同权。
fn heal_local_membership(c: &Cfg) {
    let uid = meta(K_UID);
    if uid.is_empty() || crate::collab::authority::is_member(&uid) {
        return;
    }
    let node_id = my_node_id();
    if node_id.is_empty() {
        return;
    }
    let r = fresh_assertion(c)
        .and_then(|a| crate::collab::authority::login_with_assertion_local(&a, &node_id));
    match r {
        Ok((u, _)) => {
            eprintln!("[mesh] 本机成员资格已补上(角色 {})", u.role);
            member_warn().lock().unwrap().clear();
        }
        // 补不上是有正当理由的(这台机器的主人是别人 → 要邀请码)。如实记下来给 UI 展示,
        // 但只在原因变了的时候刷屏,别每 60 秒吼一遍同一句话。
        Err(e) => {
            let mut w = member_warn().lock().unwrap();
            if *w != e {
                eprintln!("[mesh] 本机还不认这个账号:{e}");
                *w = e;
            }
        }
    }
}

/// 最近一次「进不了本机的门」的原因。空 = 没问题。UI 拿它提示用户去要一张邀请码。
fn member_warn() -> &'static Mutex<String> {
    static W: OnceLock<Mutex<String>> = OnceLock::new();
    W.get_or_init(|| Mutex::new(String::new()))
}

/// enrollment 的“有密钥”和“能双向互联”是两回事。旧版本可能已经留下 meshKey，
/// 却没有正确 pin authority 或本机成员资格；这里每次从真实持久状态推导，不相信旧成功标志。
fn enrollment_state() -> (String, bool, bool, String) {
    let Some(c) = cfg() else {
        return ("logged_out".into(), false, false, "本机还没登录".into());
    };
    let secure = authority_transport_state(&c.url) == "secure";
    let pinned = crate::collab::authority::pinned()
        .is_some_and(|(url, _, _)| url.trim_end_matches('/') == c.url.trim_end_matches('/'));
    if !pinned {
        return (
            "authority_mismatch".into(),
            false,
            secure,
            "本机信任的账号中心与设备网地址不一致，入站身份验证已阻断".into(),
        );
    }
    let uid = meta(K_UID);
    if uid.is_empty() || !crate::collab::authority::is_member(&uid) {
        let detail = member_warn().lock().unwrap().clone();
        return (
            "pending_local_admission".into(),
            false,
            secure,
            if detail.is_empty() {
                "账号已登记到设备网，但尚未成为本机成员；后台会继续自愈".into()
            } else {
                detail
            },
        );
    }
    if !secure {
        return (
            "insecure_legacy".into(),
            true,
            false,
            "当前账号中心仍使用明文 HTTP；互联可用，但登录凭据和设备密钥存在被窃取风险".into(),
        );
    }
    ("ready".into(), true, true, String::new())
}

/// 一轮对账。返回 (清单条数, 错误) —— 错误只用于日志/状态,不中断循环。
async fn reconcile_once() -> Result<usize, String> {
    orchestration::retry().await
}

async fn reconcile_peers(c: &Cfg, peers: &[Value]) -> Result<usize, String> {
    let uid = meta(K_UID);
    let mut seen: Vec<String> = Vec::new();
    for p in peers {
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
        ensure_peer(c, node_id, name, &uid).await;
        let grant = crate::collab::grants::get(node_id)?
            .ok_or_else(|| format!("设备 {node_id} 的契约未建立"))?;
        if grant.should_mount() && !links().lock().map_err(|e| e.to_string())?.contains_key(node_id) {
            return Err(format!("设备 {node_id} 的连接未建立"));
        }
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
    security::start_watch();
    rt().spawn(async { crate::collab::interconnect::sync::start(); });
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
    let grants: HashMap<String, crate::collab::grants::Grant> = crate::collab::grants::list()
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
    let links_snapshot: HashMap<String, (bool, String, String, String, u64)> = links()
        .lock()
        .unwrap()
        .iter()
        .map(|(k, l)| {
            let connected = link_connected(l);
            (
                k.clone(),
                (
                    connected,
                    l.err.clone(),
                    l.phase.clone(),
                    l.error_code.clone(),
                    l.last_verified_at,
                ),
            )
        })
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
            let nid = d
                .get("nodeId")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let g = grants.get(&nid);
            let m = mounts.get(&source_id(&nid));
            let (connected, err, phase, error_code, last_verified_at) = links_snapshot
                .get(&nid)
                .cloned()
                .unwrap_or((false, String::new(), "stopped".into(), String::new(), 0));
            d["isSelf"] = json!(d.get("self").and_then(|x| x.as_bool()).unwrap_or(nid == me));
            d["connected"] = json!(connected);
            d["phase"] = json!(phase);
            d["errorCode"] = json!(error_code);
            d["lastVerifiedAt"] = json!(last_verified_at);
            d["error"] = json!(err);
            d["drive"] = m.and_then(|x| x.get("drive")).cloned().unwrap_or(json!(""));
            d["mounted"] = m.and_then(|x| x.get("ok")).cloned().unwrap_or(json!(false));
            d["writable"] = m
                .and_then(|x| x.get("writable"))
                .cloned()
                .unwrap_or(json!(false));
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

    let uid = visible_uid();
    let (enrollment_state, ready, secure, enrollment_warn) = enrollment_state();
    Ok(json!({
        "enrolled": cfg().is_some(),
        "ready": ready,
        "secure": secure,
        "enrollmentState": enrollment_state,
        "enrollmentWarn": enrollment_warn,
        "interconnect": orchestration::status(),
        "authorityConfigured": !default_authority().is_empty(),
        "configurationError": crate::collab::interconnect::load().err(),
        "relayConfig": orchestration::relay_status(),
        "url": meta(K_URL),
        "uid": uid,
        "nodeId": me,
        "name": my_name(),
        "cloudError": cloud_msg,
        // 「这台机器认不认我」。false 时别的设备一律连不进来(它们进的就是这道门),
        // UI 据此把话说明白 + 给出邀请码那条出路,而不是让用户对着一片"看得见连不上"猜。
        "localMember": crate::collab::authority::is_member(&uid),
        "localMemberWarn": member_warn().lock().unwrap().clone(),
        "devices": devices,
    }))
}

mod policy_commands;
pub use policy_commands::*;

/// 立刻对一次账(UI 点「刷新」用,不必等下一拍心跳)。
#[cfg_attr(feature = "desktop", tauri::command)]
pub async fn mesh_sync() -> Result<Value, String> {
    let n = reconcile_once().await?;
    Ok(json!({ "peers": n }))
}

// ────────────────────────── 设备码:一码互联 ──────────────────────────
//
// 「主机」这个概念到此为止。每台机器都有一串自己的码,都能被连、也都能去连别人 ——
// 界面上不再有「设为主机」「owner 令牌」「邀请码」「入伙」这些只有我们自己懂的词。
//
// 码的形态:`PLR1-<访问码>-<NodeId>`。
// 为什么 NodeId 得在码里而不是像 UU远程 那样只给一串短数字:UU 有一台云端目录服务器
// 替它把「ID → 现在在哪」查出来,我们的云端账号中心只认**同一个账号**的设备,
// 别人的账号在上面查不到你。要做成 12 位短码,得在云机上再加一张公开的
// 「码 → NodeId」目录表并重新部署它 —— 那是另一步,得你点头。现在这串码的好处是
// **不依赖云端**:两台机器都断了公网、只在一个局域网里,照样粘码就连上。

/// 本机设备码。用户复制的就是这一串。
#[cfg_attr(feature = "desktop", tauri::command)]
pub async fn device_code() -> Result<Value, String> {
    let node_id = my_node_id();
    let code = tokio::task::spawn_blocking(crate::collab::identity::access_code)
        .await
        .map_err(|e| format!("取设备码失败:{e}"))??;
    Ok(json!({
        "code": code,
        "nodeId": node_id,
        "name": my_name(),
        // NodeId 还没就绪(隧道没起来)时给半成品:UI 照实说「稍候」,别显示一串连不上的码。
        "share": if node_id.is_empty() { String::new() } else { format!("PLR1-{code}-{node_id}") },
        "ready": !node_id.is_empty(),
    }))
}

/// 换一串码:旧码当场失效,靠旧码连进来的设备会话一并作废。
#[cfg_attr(feature = "desktop", tauri::command)]
pub async fn device_code_rotate() -> Result<Value, String> {
    let code = tokio::task::spawn_blocking(|| crate::collab::identity::rotate_access_code("本机"))
        .await
        .map_err(|e| format!("换码任务失败:{e}"))??;
    let node_id = my_node_id();
    Ok(json!({
        "code": code,
        "share": if node_id.is_empty() { String::new() } else { format!("PLR1-{code}-{node_id}") },
    }))
}

/// 解析一串设备码 → (访问码, NodeId)。只认 `PLR1-<code>-<nodeId>`。
fn parse_device_code(s: &str) -> Result<(String, String), String> {
    let t = s.trim();
    if t.to_ascii_uppercase().starts_with("PLRK1-") {
        return Err("legacy_code_unsupported:这是旧版 PLRK1 设备码，缺少一次性挑战和主机密钥证明，不能安全降级使用；请让对方升级 Polaris 后重新复制 PLR1 设备码".into());
    }
    let body = t
        .strip_prefix("PLR1-")
        .or_else(|| t.strip_prefix("plr1-"))
        .ok_or("这不是设备码 —— 它长这样:PLR1-XXXXXXXX-<一串字母数字>")?;
    let (code, node) = body
        .split_once('-')
        .ok_or("设备码不完整,少了后半段 —— 让对方整串重新复制一次")?;
    if code.trim().is_empty() || node.trim().is_empty() {
        return Err("设备码不完整 —— 让对方整串重新复制一次".into());
    }
    Ok((code.trim().to_ascii_uppercase(), node.trim().to_string()))
}

/// 设备网现状:入网了吗、名册上有谁、各自连上了没、挂成了哪块盘。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn mesh_status() -> Value {
    let enrolled = cfg().is_some();
    let (enrollment_state, ready, secure, enrollment_warn) = enrollment_state();
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
                "connected": link_connected(l),
                "phase": l.phase,
                "errorCode": l.error_code,
                "lastVerifiedAt": l.last_verified_at,
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
        "ready": ready,
        "secure": secure,
        "enrollmentState": enrollment_state,
        "enrollmentWarn": enrollment_warn,
        "interconnect": orchestration::status(),
        "authorityConfigured": !default_authority().is_empty(),
        "configurationError": crate::collab::interconnect::load().err(),
        "relayConfig": orchestration::relay_status(),
        "url": meta(K_URL),
        "uid": visible_uid(),
        "nodeId": my_node_id(),
        "name": my_name(),
        "peers": peers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_normalization_prefers_https_and_public_http_requires_consent() {
        assert_eq!(norm_url(" example.com/ "), "https://example.com");
        assert_eq!(norm_url("https://a.com/"), "https://a.com");
        assert_eq!(norm_url("http://a.com"), "http://a.com");
        assert!(validate_authority_url("http://a.com", false)
            .unwrap_err()
            .contains("明文 HTTP"));
        assert_eq!(
            validate_authority_url("http://a.com", true).unwrap(),
            "http://a.com"
        );
        assert_eq!(
            validate_authority_url("http://127.0.0.1:8080", false).unwrap(),
            "http://127.0.0.1:8080"
        );
    }

    #[test]
    fn device_code_parser_requires_the_complete_plr1_shape() {
        let (code, node) = parse_device_code("plr1-abcd2345-node-identity").unwrap();
        assert_eq!(code, "ABCD2345");
        assert_eq!(node, "node-identity");
        assert!(parse_device_code("ABCD2345").is_err());
        assert!(parse_device_code("PLR1-ABCD2345").is_err());
        assert!(parse_device_code("PLR1--node").is_err());
        assert!(parse_device_code("PLRK1-old-owner-token")
            .unwrap_err()
            .starts_with("legacy_code_unsupported:"));
    }

    #[test]
    fn logged_out_status_never_exposes_a_stale_uid() {
        assert_eq!(current_uid(false, "uid-from-old-login".into()), "");
        assert_eq!(current_uid(true, "uid-current".into()), "uid-current");
    }

    #[test]
    fn cached_token_without_a_live_tunnel_is_not_connected() {
        let link = Link {
            name: "stale".into(),
            port: 65_534,
            token: Some("cached-session".into()),
            phase: "ready".into(),
            error_code: String::new(),
            last_verified_at: now_secs(),
            err: String::new(),
        };
        assert!(
            !link_connected(&link),
            "刚验证过的 token 也不能把不存在的 tunnel 冒充成在线"
        );
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
            Link {
                name: "a".into(),
                port: p1,
                token: None,
                phase: "connecting".into(),
                error_code: String::new(),
                last_verified_at: 0,
                err: String::new(),
            },
        );
        let p2 = port_for("node-b");
        assert_ne!(p1, p2, "两台设备不能撞同一个本地口");
        assert_eq!(port_for("node-a"), p1, "已在册的必须沿用原端口");
        links().lock().unwrap().clear();
    }
}

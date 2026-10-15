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
//!    设备 proof、grant 与撤销闸。账号中心签名权一旦失陷仍是严重事件（可伪造身份），
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

mod transport;
use transport::*;

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

mod device_commands;
pub use device_commands::*;
use device_commands::parse_device_code;
mod policy_commands;
pub use policy_commands::*;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tls_tests;

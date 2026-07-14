//! 手机端 iroh 隧道原生库(路线 B)。逻辑镜像桌面 collab/tunnel.rs 的成员端:
//! 本机 127.0.0.1:<port> 每条 TCP 连接 → 一条到主机 NodeId 的 iroh 双向流(打洞直连,
//! 失败走 n0 relay 兜底)。WebView 把 base 设成 http://127.0.0.1:<port> 即经隧道访问桌面。
//!
//! 经 JNI 暴露给 Capacitor 插件(com.polaris.mobile.PolarisTunnel):
//!  - deviceNodeId(dataDir)              → 本机设备 NodeId(主机据此加白名单)
//!  - startClient(dataDir,hostNid,port)  → 起隧道,返回 "" 或错误串
//!  - stopClient()                       → 停
//!  - tunnelState()                      → JSON {running,state,last_error}

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use base64::Engine;
use iroh::endpoint::presets;
use iroh::endpoint::{Connection, IdleTimeout, QuicTransportConfig};
use iroh::{Endpoint, EndpointId, SecretKey};
use once_cell::sync::Lazy;

const ALPN: &[u8] = b"polaris/1";
const KEEP_ALIVE: Duration = Duration::from_secs(15);
const IDLE_TIMEOUT: Duration = Duration::from_secs(60);

static RUNNING: AtomicBool = AtomicBool::new(false);
static SHUTDOWN: Lazy<tokio::sync::Notify> = Lazy::new(tokio::sync::Notify::new);
static STATE: Lazy<Mutex<String>> = Lazy::new(|| Mutex::new("stopped".into()));
static LAST_ERROR: Lazy<Mutex<String>> = Lazy::new(|| Mutex::new(String::new()));
static CLIENT_CONN: Lazy<Mutex<Option<Connection>>> = Lazy::new(|| Mutex::new(None));

fn set_state(s: &str) {
    *STATE.lock().unwrap() = s.to_string();
}
fn set_error(e: impl Into<String>) {
    *LAST_ERROR.lock().unwrap() = e.into();
}

// ── 设备密钥(<data_dir>/device.key,base64url no-pad 的 32 字节种子) ──────────

fn device_key_path(data_dir: &str) -> PathBuf {
    PathBuf::from(data_dir).join("device.key")
}

fn get_or_create_device_key(data_dir: &str) -> Result<[u8; 32], String> {
    let path = device_key_path(data_dir);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("建目录失败: {e}"))?;
    }
    if path.exists() {
        let raw = std::fs::read_to_string(&path).map_err(|e| format!("读 device.key 失败: {e}"))?;
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(raw.trim())
            .map_err(|e| format!("device.key 损坏: {e}"))?;
        if bytes.len() != 32 {
            return Err("device.key 长度异常".into());
        }
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&bytes);
        return Ok(seed);
    }
    let mut seed = [0u8; 32];
    getrandom::getrandom(&mut seed).map_err(|e| format!("生成密钥失败: {e}"))?;
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(seed);
    std::fs::write(&path, encoded.as_bytes()).map_err(|e| format!("写 device.key 失败: {e}"))?;
    Ok(seed)
}

fn node_id_of(seed: &[u8; 32]) -> String {
    SecretKey::from_bytes(seed).public().to_string()
}

// ── iroh Endpoint + 客户端代理(镜像 tunnel.rs client_proxy) ──────────────────

async fn build_endpoint(seed: [u8; 32]) -> Result<Endpoint, String> {
    let transport = QuicTransportConfig::builder()
        .keep_alive_interval(KEEP_ALIVE)
        .max_idle_timeout(Some(
            IdleTimeout::try_from(IDLE_TIMEOUT).expect("idle timeout 常量合法"),
        ))
        .build();
    Endpoint::builder(presets::N0)
        .secret_key(SecretKey::from_bytes(&seed))
        .transport_config(transport)
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await
        .map_err(|e| format!("iroh endpoint 绑定失败: {e}"))
}

async fn healthy_conn(ep: &Endpoint, host_id: EndpointId) -> Result<Connection, String> {
    let cached = CLIENT_CONN.lock().unwrap().clone();
    if let Some(c) = cached {
        if c.close_reason().is_none() {
            return Ok(c);
        }
        *CLIENT_CONN.lock().unwrap() = None;
    }
    set_state("connecting");
    let c = ep
        .connect(host_id, ALPN)
        .await
        .map_err(|e| format!("连主机失败: {e}"))?;
    *CLIENT_CONN.lock().unwrap() = Some(c.clone());
    set_state("connected");
    set_error("");
    Ok(c)
}

async fn client_proxy(host_node_id: String, listen_port: u16, seed: [u8; 32]) -> Result<(), String> {
    let host_id: EndpointId = host_node_id
        .trim()
        .parse()
        .map_err(|e| format!("主机 NodeId 非法: {e}"))?;
    set_state("connecting");
    let ep = build_endpoint(seed).await.inspect_err(|e| {
        set_error(e.clone());
        set_state("stopped");
    })?;
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", listen_port))
        .await
        .map_err(|e| format!("本地端口 {listen_port} 监听失败: {e}"))?;
    RUNNING.store(true, Ordering::SeqCst);

    // 后台健康巡检:连接死了主动重连,退避封顶 30s。
    let watchdog = {
        let ep = ep.clone();
        tokio::spawn(async move {
            let mut backoff = Duration::from_secs(1);
            loop {
                tokio::time::sleep(Duration::from_secs(10)).await;
                if !RUNNING.load(Ordering::SeqCst) {
                    break;
                }
                let dead = CLIENT_CONN
                    .lock()
                    .unwrap()
                    .as_ref()
                    .map(|c| c.close_reason().is_some())
                    .unwrap_or(false);
                if !dead {
                    backoff = Duration::from_secs(1);
                    continue;
                }
                set_state("reconnecting");
                loop {
                    match healthy_conn(&ep, host_id).await {
                        Ok(_) => {
                            backoff = Duration::from_secs(1);
                            break;
                        }
                        Err(e) => {
                            set_error(&e);
                            tokio::time::sleep(backoff).await;
                            backoff = (backoff * 2).min(Duration::from_secs(30));
                            if !RUNNING.load(Ordering::SeqCst) {
                                return;
                            }
                        }
                    }
                }
            }
        })
    };

    if let Err(e) = healthy_conn(&ep, host_id).await {
        set_error(&e);
    }

    loop {
        let (mut tcp, _peer) = tokio::select! {
            _ = SHUTDOWN.notified() => break,
            acc = listener.accept() => match acc {
                Ok(a) => a,
                Err(_) => continue,
            },
        };
        let bi = match healthy_conn(&ep, host_id).await {
            Ok(c) => match c.open_bi().await {
                Ok(s) => Some(s),
                Err(_) => {
                    *CLIENT_CONN.lock().unwrap() = None;
                    match healthy_conn(&ep, host_id).await {
                        Ok(c2) => c2.open_bi().await.ok(),
                        Err(_) => None,
                    }
                }
            },
            Err(e) => {
                set_error(&e);
                None
            }
        };
        let Some((send, recv)) = bi else { continue };
        tokio::spawn(async move {
            let mut stream = tokio::io::join(recv, send);
            let _ = tokio::io::copy_bidirectional(&mut tcp, &mut stream).await;
        });
    }

    RUNNING.store(false, Ordering::SeqCst);
    set_state("stopped");
    watchdog.abort();
    *CLIENT_CONN.lock().unwrap() = None;
    ep.close().await;
    Ok(())
}

/// 起客户端(独立线程 + 独立 tokio runtime)。已在跑则先停旧的。
fn start(data_dir: &str, host_node_id: &str, listen_port: u16) -> Result<(), String> {
    if RUNNING.load(Ordering::SeqCst) {
        stop();
        std::thread::sleep(Duration::from_millis(200));
    }
    let seed = get_or_create_device_key(data_dir)?;
    let host = host_node_id.to_string();
    std::thread::Builder::new()
        .name("polaris-mobile-tunnel".into())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    set_error(format!("runtime 创建失败: {e}"));
                    set_state("stopped");
                    return;
                }
            };
            if let Err(e) = rt.block_on(client_proxy(host, listen_port, seed)) {
                set_error(e);
                set_state("stopped");
            }
        })
        .map_err(|e| format!("起隧道线程失败: {e}"))?;
    Ok(())
}

fn stop() {
    RUNNING.store(false, Ordering::SeqCst);
    SHUTDOWN.notify_waiters();
    set_state("stopped");
}

// ── JNI 桥(com.polaris.mobile.PolarisTunnel) ────────────────────────────────

use jni::objects::{JClass, JString};
use jni::sys::{jint, jstring};
use jni::JNIEnv;

fn jstr(env: &mut JNIEnv, s: &JString) -> String {
    env.get_string(s).map(|v| v.into()).unwrap_or_default()
}
fn ret(env: &JNIEnv, s: &str) -> jstring {
    env.new_string(s).map(|o| o.into_raw()).unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub extern "system" fn Java_com_polaris_mobile_PolarisTunnel_deviceNodeId(
    mut env: JNIEnv,
    _class: JClass,
    data_dir: JString,
) -> jstring {
    let dir = jstr(&mut env, &data_dir);
    match get_or_create_device_key(&dir) {
        Ok(seed) => ret(&env, &node_id_of(&seed)),
        Err(e) => ret(&env, &format!("ERR:{e}")),
    }
}

#[no_mangle]
pub extern "system" fn Java_com_polaris_mobile_PolarisTunnel_startClient(
    mut env: JNIEnv,
    _class: JClass,
    data_dir: JString,
    host_node_id: JString,
    listen_port: jint,
) -> jstring {
    let dir = jstr(&mut env, &data_dir);
    let host = jstr(&mut env, &host_node_id);
    match start(&dir, &host, listen_port as u16) {
        Ok(()) => ret(&env, ""),
        Err(e) => ret(&env, &e),
    }
}

#[no_mangle]
pub extern "system" fn Java_com_polaris_mobile_PolarisTunnel_stopClient(
    _env: JNIEnv,
    _class: JClass,
) {
    stop();
}

#[no_mangle]
pub extern "system" fn Java_com_polaris_mobile_PolarisTunnel_tunnelState(
    env: JNIEnv,
    _class: JClass,
) -> jstring {
    let v = serde_json::json!({
        "running": RUNNING.load(Ordering::SeqCst),
        "state": STATE.lock().unwrap().clone(),
        "last_error": LAST_ERROR.lock().unwrap().clone(),
    });
    ret(&env, &v.to_string())
}

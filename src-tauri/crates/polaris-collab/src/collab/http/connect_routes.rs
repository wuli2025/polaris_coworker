use super::*;

// ───────────────────────── 设备码:一码进门 ─────────────────────────
//
// 这一组取代了「邀请码 / 票据 / owner 令牌连接码」三条路。两边完全对称:每台机器都有
// 自己的码,谁把码给谁,谁就能连过去 —— 界面上不再有「主机」和「被邀请者」之分。
//
// 进门是**公开**端点(不能要求先鉴权 —— 对方就是因为没凭据才来敲门的),所以这里是
// 整个协作面唯一一个凭一串短码放人进来的地方,三道自保必须都在:
//  ① 码由本机验,验的是 meta 里那串常驻码(可随时换,换了旧码当场作废);
//  ② 进来拿到的是 **visitor** 会话 + 一行**默认只读**的设备契约,不是 owner;
//  ③ 失败要计次 —— 8 位码 32^8,不限速的话在线爆破不是天方夜谭。

const CONNECT_CHALLENGE_CAP: usize = 1024;
const CONNECT_CHALLENGE_TTL: Duration = Duration::from_secs(60);
const CONNECT_CHALLENGE_ERROR: &str = "连接挑战无效或已过期，请重新发起连接";

struct PendingConnectChallenge {
    node_id: String,
    expires_at: Instant,
}

struct ConnectChallengeStore {
    entries: HashMap<String, PendingConnectChallenge>,
    capacity: usize,
    ttl: Duration,
}

impl ConnectChallengeStore {
    fn new(capacity: usize, ttl: Duration) -> Self {
        Self {
            entries: HashMap::new(),
            capacity: capacity.max(1),
            ttl,
        }
    }

    fn prune(&mut self, now: Instant) {
        self.entries.retain(|_, pending| pending.expires_at > now);
    }

    fn insert(&mut self, challenge: String, node_id: String, now: Instant) {
        self.prune(now);
        if self.entries.len() >= self.capacity {
            let oldest = self
                .entries
                .iter()
                .min_by(|(key_a, a), (key_b, b)| {
                    a.expires_at
                        .cmp(&b.expires_at)
                        .then_with(|| key_a.cmp(key_b))
                })
                .map(|(key, _)| key.clone());
            if let Some(key) = oldest {
                self.entries.remove(&key);
            }
        }
        self.entries.insert(
            challenge,
            PendingConnectChallenge {
                node_id,
                expires_at: now + self.ttl,
            },
        );
    }

    /// 查到即删除：NodeId 不匹配、签名错误、设备码错误或后续数据库失败都不能重放。
    fn consume(&mut self, challenge: &str, node_id: &str, now: Instant) -> Result<(), String> {
        let Some(pending) = self.entries.remove(challenge) else {
            return Err(CONNECT_CHALLENGE_ERROR.into());
        };
        if pending.expires_at <= now || pending.node_id != node_id {
            return Err(CONNECT_CHALLENGE_ERROR.into());
        }
        Ok(())
    }
}

fn connect_challenges() -> &'static std::sync::Mutex<ConnectChallengeStore> {
    static CHALLENGES: std::sync::OnceLock<std::sync::Mutex<ConnectChallengeStore>> =
        std::sync::OnceLock::new();
    CHALLENGES.get_or_init(|| {
        std::sync::Mutex::new(ConnectChallengeStore::new(
            CONNECT_CHALLENGE_CAP,
            CONNECT_CHALLENGE_TTL,
        ))
    })
}

pub(super) fn issue_connect_challenge(node_id: String) -> Result<String, String> {
    let mut store = connect_challenges()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    for _ in 0..4 {
        let mut random = [0u8; 32];
        getrandom::getrandom(&mut random).map_err(|e| format!("生成连接挑战失败: {e}"))?;
        let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(random);
        if !store.entries.contains_key(&challenge) {
            store.insert(challenge.clone(), node_id, Instant::now());
            return Ok(challenge);
        }
    }
    Err("生成连接挑战失败，请重试".into())
}

/// 第一步只发一个短命、一次性的随机数，并把它绑定到规范化 NodeId。它不是授权，公开可取；
/// 真正进门仍须用对应 host.key 签名，再同时通过设备码校验。
pub(super) async fn collab_connect_challenge(Json(v): Json<Value>) -> Response {
    let node_id = match crate::collab::identity::canonical_node_id(&s_of(&v, "nodeId")) {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response(),
    };
    match issue_connect_challenge(node_id) {
        Ok(challenge) => Json(json!({
            "challenge": challenge,
            "expiresIn": CONNECT_CHALLENGE_TTL.as_secs(),
        }))
        .into_response(),
        Err(e) => err_resp(e),
    }
}

/// 凭设备码进门。回一张本机会话 + 这台设备当前的档位。
///
/// 幂等:同一台设备重复连不会重置机主调过的档位(`grant_by_code` 见到已有契约原样返回)。
pub(super) async fn collab_connect(headers: HeaderMap, Json(v): Json<Value>) -> Response {
    let peer = peer_ip_of(&headers);
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let node_id = crate::collab::identity::canonical_node_id(&s_of(&v, "nodeId"))?;
        connect_throttle(&peer)?;

        let challenge = s_of(&v, "challenge");
        let consumed = connect_challenges()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .consume(challenge.trim(), &node_id, Instant::now());
        if let Err(e) = consumed {
            crate::collab::db::audit("anonymous", "connect.denied", &node_id, &e);
            return Err(CONNECT_CHALLENGE_ERROR.into());
        }

        let code = s_of(&v, "code");
        if let Err(e) = crate::collab::identity::verify_connect_proof(
            &node_id,
            &code,
            &challenge,
            &s_of(&v, "signature"),
        ) {
            crate::collab::db::audit("anonymous", "connect.denied", &node_id, &e);
            return Err("无法证明这台设备持有其 NodeId，请重新发起连接".into());
        }
        if !crate::collab::identity::check_access_code(&code)? {
            crate::collab::db::audit("anonymous", "connect.denied", &node_id, "设备码不对");
            return Err("设备码不对 —— 让对方在他的「互联」页重新复制一次(他可能换过码)".into());
        }
        connect_throttle_reset(&peer);
        let name = {
            let n = s_of(&v, "name");
            if n.trim().is_empty() {
                "对方设备".to_string()
            } else {
                n
            }
        };
        let g = crate::collab::grants::grant_by_code(&node_id, &name)?;
        let user = crate::collab::auth::peer_guest_user()?;
        let token = crate::collab::auth::issue_session(user.id, &node_id)?;
        crate::collab::db::audit(&name, "connect.ok", &node_id, g.effective_fs());
        Ok(json!({
            "token": token,
            "user": user,
            "fsAccess": g.effective_fs(),
            "execAccess": g.effective_exec(),
            // 对方 UI 直接照这句说话,不必自己拼:一进来就说清「现在只读,等他放行」。
            "note": if g.effective_fs() == crate::collab::grants::FS_RW {
                "已连上,对方给了完整权限"
            } else {
                "已连上。默认只读 —— 等对方在他的「设备与授权」里点「信任这台」才能写入/跑命令"
            },
        }))
    })
    .await;
    unwrap_api(out)
}

/// 本机的设备码(owner 才看得到 —— 码就是钥匙,不能对未鉴权的请求吐)。
pub(super) async fn collab_code_get(State(state): State<CollabState>, headers: HeaderMap) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    if role_rank(&ctx.role) < 3 {
        return forbid();
    }
    let out = tokio::task::spawn_blocking(|| {
        crate::collab::identity::access_code().map(|c| json!({ "code": c }))
    })
    .await;
    unwrap_api(out)
}

/// 换一串码。旧码当场失效,靠旧码进来的设备会话一并作废。
pub(super) async fn collab_code_rotate(State(state): State<CollabState>, headers: HeaderMap) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    if role_rank(&ctx.role) < 3 {
        return forbid();
    }
    let actor = ctx.username.clone();
    let out = tokio::task::spawn_blocking(move || {
        crate::collab::identity::rotate_access_code(&actor).map(|c| json!({ "code": c }))
    })
    .await;
    unwrap_api(out)
}

/// 敲门失败计次(按来源)。8 位码 32^8 ≈ 1.1e12,但不限速的话分布式在线爆破仍然可行 ——
/// 而这道门后面是**别人的硬盘**。连错 10 次即锁 10 分钟,锁的是来源不是码本身
/// (锁码等于给了任何人一个把机主自己也关在外面的开关)。
pub(super) fn connect_fails() -> &'static std::sync::Mutex<HashMap<String, (u32, i64)>> {
    static F: std::sync::OnceLock<std::sync::Mutex<HashMap<String, (u32, i64)>>> =
        std::sync::OnceLock::new();
    F.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

const CONNECT_MAX_FAILS: u32 = 10;
const CONNECT_LOCK_SECS: i64 = 600;
const CONNECT_THROTTLE_CAP: usize = 4096;

pub(super) fn connect_throttle(peer: &str) -> Result<(), String> {
    let mut m = connect_fails().lock().unwrap_or_else(|e| e.into_inner());
    let now = crate::collab::db::now();
    // 转发头不是可信身份，攻击者可以不断换值；频控表必须有硬上限，不能让公开端点
    // 靠伪造 X-Forwarded-For 把进程内存一直顶高。
    m.retain(|_, (_, last)| now - *last < CONNECT_LOCK_SECS);
    if !m.contains_key(peer) && m.len() >= CONNECT_THROTTLE_CAP {
        if let Some(oldest) = m
            .iter()
            .min_by_key(|(_, (_, last))| *last)
            .map(|(key, _)| key.clone())
        {
            m.remove(&oldest);
        }
    }
    let e = m.entry(peer.to_string()).or_insert((0, 0));
    if e.0 >= CONNECT_MAX_FAILS && now - e.1 < CONNECT_LOCK_SECS {
        return Err(format!(
            "连错太多次,请 {} 分钟后再试",
            (CONNECT_LOCK_SECS - (now - e.1)) / 60 + 1
        ));
    }
    if now - e.1 >= CONNECT_LOCK_SECS {
        *e = (0, 0);
    }
    e.0 += 1;
    e.1 = now;
    Ok(())
}

pub(super) fn connect_throttle_reset(peer: &str) {
    connect_fails()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(peer);
}

/// 请求来源标识。经 iroh 隧道进来的对端都是 127.0.0.1,所以还带上转发头 ——
/// 取不到就退回一个固定桶(那时限速是全局的,宁可误伤也不放开)。
pub(super) fn peer_ip_of(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .or_else(|| headers.get("x-real-ip"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "tunnel".to_string())
}

#[cfg(test)]
mod connect_challenge_tests {
    use super::*;

    #[test]
    fn challenge_is_single_use_and_node_bound() {
        let now = Instant::now();
        let mut store = ConnectChallengeStore::new(8, Duration::from_secs(60));
        store.insert("once".into(), "node-a".into(), now);
        assert!(store.consume("once", "node-a", now).is_ok());
        assert!(store.consume("once", "node-a", now).is_err());

        store.insert("bound".into(), "node-a".into(), now);
        assert!(store.consume("bound", "node-b", now).is_err());
        assert!(
            store.consume("bound", "node-a", now).is_err(),
            "NodeId 不匹配也必须烧掉挑战"
        );
    }

    #[test]
    fn challenge_expires_and_store_stays_bounded() {
        let now = Instant::now();
        let mut store = ConnectChallengeStore::new(2, Duration::from_secs(10));
        store.insert("oldest".into(), "node-a".into(), now);
        store.insert(
            "middle".into(),
            "node-b".into(),
            now + Duration::from_secs(1),
        );
        store.insert(
            "newest".into(),
            "node-c".into(),
            now + Duration::from_secs(2),
        );
        assert_eq!(store.entries.len(), 2);
        assert!(
            !store.entries.contains_key("oldest"),
            "满载时应淘汰最早到期项"
        );

        assert!(store
            .consume("middle", "node-b", now + Duration::from_secs(12))
            .is_err());
        store.prune(now + Duration::from_secs(13));
        assert!(store.entries.is_empty());
    }
}

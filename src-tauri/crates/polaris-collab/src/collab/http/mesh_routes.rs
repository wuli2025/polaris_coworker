//! Authority mesh directory HTTP handlers.
use super::*;

fn mesh_key_of(headers: &HeaderMap) -> Result<String, Response> {
    bearer_of(headers).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"缺设备密钥(Bearer mk_…)"})),
        )
            .into_response()
    })
}

/// 设备入网:拿一张身份断言换一把长期设备密钥。
///
/// 断言由本机(权威)自己签的,所以这里用**自己的公钥**验 —— 等于确认「这张断言真是我签的、
/// 没过期」。客户端的断言来自 `/api/account/login`,即用户刚亲自输过密码。
pub(super) async fn mesh_enroll(Json(v): Json<Value>) -> Response {
    if !crate::collab::authority::is_authority() {
        return not_authority();
    }
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        use crate::collab::authority as A;
        let claims = A::verify_assertion_with(&s_of(&v, "assertion"), &A::public_key_b64()?)?;
        crate::collab::interconnect::binding::node(&claims, &s_of(&v, "nodeId"))?;
        let key = crate::collab::mesh::enroll(
            &claims.uid,
            &s_of(&v, "nodeId"),
            &s_of(&v, "name"),
            &s_of(&v, "os"),
            &s_of(&v, "ver"),
        )?;
        Ok(json!({ "meshKey": key, "uid": claims.uid, "username": claims.username,
            "relays": crate::collab::interconnect::advertised_relays()? }))
    })
    .await;
    unwrap_api(out)
}

/// 心跳 + 取同账号设备清单(一次往返办两件事)。
pub(super) async fn mesh_announce(headers: HeaderMap, Json(v): Json<Value>) -> Response {
    if !crate::collab::authority::is_authority() {
        return not_authority();
    }
    let key = match mesh_key_of(&headers) {
        Ok(k) => k,
        Err(r) => return r,
    };
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let (uid, peers) = crate::collab::mesh::announce(
            &key,
            &s_of(&v, "name"),
            &s_of(&v, "os"),
            &s_of(&v, "ver"),
        )?;
        let (_, node) = crate::collab::mesh::resolve_key(&key)?;
        Ok(json!({ "uid": uid, "peers": peers,
            "notifications": crate::collab::interconnect::notifications::inbox(&uid, &node)?,
            "approvals": crate::collab::interconnect::risk::pending(&uid, &node)?,
            "relays": crate::collab::interconnect::advertised_relays()? }))
    })
    .await;
    unwrap_api(out)
}

/// 设备密钥 → 一张新的 5 分钟身份断言。设备拿它去**对端主机**换本机会话。
///
/// 这是设备网唯一的「凭据升级」点,也是它必须只能自助的原因:签出来的断言只说
/// 「我是 uid X」,进不进得去某台机器仍由那台机器自己的成员资格决定。
pub(super) async fn mesh_assert(headers: HeaderMap) -> Response {
    if !crate::collab::authority::is_authority() {
        return not_authority();
    }
    let key = match mesh_key_of(&headers) {
        Ok(k) => k,
        Err(r) => return r,
    };
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let (uid, node) = crate::collab::mesh::resolve_key(&key)?;
        let (username, _, _) = crate::collab::mesh::account_of(&uid)?;
        let assertion = crate::collab::authority::sign_device_assertion(&uid, &node)?;
        Ok(json!({ "assertion": assertion, "uid": uid, "username": username }))
    })
    .await;
    unwrap_api(out)
}

/// 设备**台账**:这个账号名下的全部设备(含自己那台、含已移出的)。
///
/// 与 announce 回的名册的分工:那个是给后台对账循环用的「我该连谁」,这个是给人看的。
/// 台账少一台都是缺陷 —— 用户要认出「哪台是我正用的」,也要看见「上周被我踢掉的那台」。
pub(super) async fn mesh_devices(headers: HeaderMap) -> Response {
    if !crate::collab::authority::is_authority() {
        return not_authority();
    }
    let key = match mesh_key_of(&headers) {
        Ok(k) => k,
        Err(r) => return r,
    };
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let (uid, me) = crate::collab::mesh::resolve_key(&key)?;
        let list: Vec<Value> = crate::collab::mesh::devices(&uid)?
            .into_iter()
            .map(|(p, revoked)| {
                json!({
                    "nodeId": p.node_id, "name": p.name, "os": p.os, "ver": p.ver,
                    "lastSeen": p.last_seen, "online": p.online, "firstSeen": p.first_seen,
                    "revoked": revoked,
                    // 「这台就是你现在坐着的这台」——台账上要标出来,不然一屋子机器名认不出谁是谁。
                    "self": p.node_id == me,
                })
            })
            .collect();
        Ok(json!({ "uid": uid, "nodeId": me, "devices": list }))
    })
    .await;
    unwrap_api(out)
}

/// 给自己账号名下的设备改个人话名字。改过之后心跳不再用机器名覆盖它。
pub(super) async fn mesh_rename(headers: HeaderMap, Json(v): Json<Value>) -> Response {
    if !crate::collab::authority::is_authority() {
        return not_authority();
    }
    let key = match mesh_key_of(&headers) {
        Ok(k) => k,
        Err(r) => return r,
    };
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let (uid, _) = crate::collab::mesh::resolve_key(&key)?;
        crate::collab::mesh::rename(&uid, &s_of(&v, "nodeId"), &s_of(&v, "name"))?;
        Ok(json!({ "ok": true }))
    })
    .await;
    unwrap_api(out)
}

pub(super) async fn mesh_revoke(headers: HeaderMap, Json(v): Json<Value>) -> Response {
    if !crate::collab::authority::is_authority() {
        return not_authority();
    }
    let key = match mesh_key_of(&headers) {
        Ok(k) => k,
        Err(r) => return r,
    };
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let (uid, node) = crate::collab::mesh::resolve_key(&key)?;
        crate::collab::interconnect::gate::require(&uid, &node, "revoke_other_device", &v)?;
        crate::collab::mesh::revoke(&uid, &s_of(&v, "nodeId"))?;
        Ok(json!({ "ok": true }))
    })
    .await;
    unwrap_api(out)
}

/// Signed documents are public verification data; clients must verify against their pinned key.
pub(super) async fn mesh_revocations(Query(q): Query<HashMap<String, String>>) -> Response {
    if !crate::collab::authority::is_authority() { return not_authority(); }
    let Some(uid) = q.get("uid").filter(|uid| !uid.is_empty() && uid.len() <= 256).cloned() else {
        return err_resp("缺账号 UID".into());
    };
    unwrap_api(tokio::task::spawn_blocking(move || -> Result<Value, String> {
        Ok(json!({"document": crate::collab::interconnect::revocations::signed(&uid)?}))
    }).await)
}

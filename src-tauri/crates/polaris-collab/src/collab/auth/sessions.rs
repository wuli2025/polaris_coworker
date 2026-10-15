//! Session validation and short-cache access.
use super::*;

/// 校验会话 token → 返回用户（check_auth 的核心）。过期或吊销即失败。
/// 短缓存命中(TTL 内且吊销版本未变)直接返回,免开库;未命中走每线程复用连接查库。
pub fn check_session(token: &str) -> Result<User, String> {
    check_session_with_device(token).map(|(u, _)| u)
}

/// 同 [`check_session`],但把会话上记的**设备标识**一并带出来。
///
/// 入方向的按设备闸(`grants::fs_gate` / `grants::exec_gate`)靠它认出「这条请求
/// 是哪台设备发来的」—— 没有它,台账上那两排权限档对连进来的人就是摆设。
pub fn check_session_with_device(token: &str) -> Result<(User, String), String> {
    crate::collab::interconnect::binding::check(token)?;
    let t = now();
    // 版本号必须在查缓存/查库**之前**读:若查询期间发生吊销,写入缓存的条目带旧版本号,
    // 下次请求即判陈旧 —— 堵住「查到旧数据 + 吊销后才写缓存」的竞态窗口。
    let gen = SESSION_REVOKE_GEN.load(Ordering::SeqCst);
    // 当前库路径:缓存按库隔离(切库不串库)。取不到路径就用固定串,行为退化为不按库隔离但不 panic。
    let cur_db = db::db_path()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    {
        let mut cache = SESSION_CACHE.lock();
        if let Some(e) = cache.get(token) {
            // 命中四道闸:版本未变 + 同库 + TTL 内 + 会话自身未到期(缺一即落库重查)。
            if e.gen == gen
                && e.db_path == cur_db
                && t - e.cached_at < SESSION_CACHE_TTL_SECS
                && t < e.expires_at
            {
                return Ok((e.user.clone(), e.device_id.clone()));
            }
            cache.remove(token); // 陈旧 → 惰性剔除,落库重查
        }
    }
    let (user, device_id, expires_at) = check_session_db(token, t)?;
    let mut cache = SESSION_CACHE.lock();
    // 硬上限防 token 洪泛撑爆内存:先剔陈旧;仍超限就整体清空(纯性能缓存,清空只是回落到查库)。
    if cache.len() >= SESSION_CACHE_CAP {
        cache.retain(|_, e| {
            e.gen == gen && t - e.cached_at < SESSION_CACHE_TTL_SECS && t < e.expires_at
        });
        if cache.len() >= SESSION_CACHE_CAP {
            cache.clear();
        }
    }
    cache.insert(
        token.to_string(),
        CachedSession {
            user: user.clone(),
            device_id: device_id.clone(),
            expires_at,
            cached_at: t,
            gen,
            db_path: cur_db,
        },
    );
    Ok((user, device_id))
}

/// check_session 的落库部分(原逻辑不变,连接改走每线程复用的 with_conn)。
/// 返回 (用户, 设备标识, 会话到期时刻)。
fn check_session_db(token: &str, t: i64) -> Result<(User, String, i64), String> {
    db::with_conn(|conn| {
        let row = conn.query_row(
            "SELECT u.id,u.username,u.role,u.display_name,u.disabled,s.expires_at,COALESCE(b.node_id,s.device_id) \
             FROM sessions s JOIN users u ON u.id=s.user_id LEFT JOIN bound_sessions b ON b.token=s.token WHERE s.token=?1",
            params![token],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, i64>(5)?,
                    r.get::<_, String>(6)?,
                ))
            },
        );
        let (id, username, role, display_name, disabled, expires_at, device_id) =
            row.map_err(|_| "会话无效，请重新登录".to_string())?;
        if disabled != 0 {
            return Err("账号已停用".into());
        }
        if expires_at < t {
            let _ = conn.execute("DELETE FROM sessions WHERE token=?1", params![token]);
            return Err("会话已过期，请重新登录".into());
        }
        Ok((
            User {
                id,
                username,
                role,
                display_name,
                disabled: false,
            },
            device_id,
            expires_at,
        ))
    })
}


//! Identity tickets operations; public paths are re-exported by identity.
use super::*;

#[derive(serde::Serialize)]
pub struct Ticket {
    pub code: String,
    pub role: String,
    pub expires_at: i64,
}

/// 签发一次性邀请票据。role 决定兑换后账号的角色。
pub fn create_ticket(role: &str, note: &str) -> Result<Ticket, String> {
    if !matches!(role, "collaborator" | "visitor") {
        return Err(
            "邀请票据只允许 collaborator 或 visitor；owner 必须通过单独的审计晋升流程授予".into(),
        );
    }
    if note.chars().count() > 200 || note.chars().any(char::is_control) {
        return Err("票据备注最多 200 字且不能包含控制字符".into());
    }
    let conn = open_db()?;
    let code = random_code();
    let t = now();
    conn.execute(
        "INSERT INTO tickets(code,role,created_at,expires_at,note) VALUES(?1,?2,?3,?4,?5)",
        params![code, role, t, t + TICKET_TTL, note],
    )
    .map_err(|e| format!("签发票据失败: {e}"))?;
    db::audit("owner", "ticket.create", role, note);
    Ok(Ticket {
        code,
        role: role.into(),
        expires_at: t + TICKET_TTL,
    })
}

/// 兑换票据 → 建账号 + 登记设备。票据用后即废（幂等：并发下唯一 UPDATE 保证只成一次）。
pub fn redeem_ticket(
    code: &str,
    username: &str,
    password: &str,
    display_name: &str,
    device_name: &str,
    node_id: &str,
) -> Result<(auth::User, String), String> {
    // 先做廉价输入校验并确认票据真实有效，再计算昂贵 Argon2；否则未认证攻击者拿随机
    // 无效码即可让每个请求烧一次密码哈希 CPU。事务内还会再次读取并 CAS，防 TOCTOU。
    auth::validate_new_account_input(username, password, display_name)?;
    crate::collab::device_label::validate_characters(device_name)?;
    let device_name = device_name.trim();
    let node_id = node_id.trim();
    if device_name.is_empty()
        || device_name.chars().count() > 80
        || device_name.chars().any(char::is_control)
    {
        return Err("设备名称长度须为 1–80 个字符且不能包含控制字符".into());
    }
    if node_id.is_empty() || node_id.len() > 256 || node_id.chars().any(char::is_control) {
        return Err("设备标识无效".into());
    }
    {
        let conn = open_db()?;
        let (role, expires_at, used_at) = conn
            .query_row(
                "SELECT role,expires_at,used_at FROM tickets WHERE code=?1",
                params![code.trim()],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, Option<i64>>(2)?,
                    ))
                },
            )
            .map_err(|_| "票据无效".to_string())?;
        if used_at.is_some() {
            return Err("票据已被使用".into());
        }
        if expires_at < now() {
            return Err("票据已过期".into());
        }
        if !matches!(role.as_str(), "member" | "collaborator" | "visitor") {
            return Err("该票据角色已停用，请管理员重新签发最小权限票据".into());
        }
    }
    let pass_hash = auth::prepare_new_account(username, password, display_name)?;
    let device_id = random_device_id()?;
    let token = auth::random_token();
    let mut conn = open_db()?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| format!("兑换事务失败: {e}"))?;
    let row = tx.query_row(
        "SELECT role,expires_at,used_at FROM tickets WHERE code=?1",
        params![code.trim()],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, Option<i64>>(2)?,
            ))
        },
    );
    let (mut role, expires_at, used_at) = row.map_err(|_| "票据无效".to_string())?;
    // 兼容旧版 UI 签出的 member 票据；其它高权限旧票据 fail-closed。
    if role == "member" {
        role = "collaborator".into();
    }
    if !matches!(role.as_str(), "collaborator" | "visitor") {
        return Err("该票据角色已停用，请管理员重新签发最小权限票据".into());
    }
    if used_at.is_some() {
        return Err("票据已被使用".into());
    }
    if expires_at < now() {
        return Err("票据已过期".into());
    }
    // 先原子占用票据，防并发重复兑换。used_by 记下兑换账号(管理面可溯源)。
    let claimed = tx
        .execute(
            "UPDATE tickets SET used_at=?1, used_by=?3 WHERE code=?2 AND used_at IS NULL",
            params![now(), code.trim(), username],
        )
        .map_err(|e| e.to_string())?;
    if claimed != 1 {
        return Err("票据已被使用".into());
    }
    let user = auth::insert_user_tx(&tx, username, &pass_hash, &role, display_name)?;
    tx.execute(
        "INSERT INTO devices(id,user_id,name,node_id,added_at) VALUES(?1,?2,?3,?4,?5)",
        params![device_id, user.id, device_name, node_id, now()],
    )
    .map_err(|e| format!("登记设备失败: {e}"))?;
    auth::insert_session_tx(&tx, &token, user.id, node_id)?;
    tx.commit().map_err(|e| format!("提交兑换失败: {e}"))?;
    db::audit(username, "ticket.redeem", code.trim(), &role);
    Ok((user, token))
}

/// 已登录账号凭邀请码入伙:**不建新账号**,只把这台设备绑到当前账号并核销票据。
/// 用户要求的「同账号的人通过邀请码进来」—— 票据上记 used_by=该账号,全程可溯源。
/// 返回设备 id。node_id 已属别的账号会被 ensure_device_for 拒绝(防顶替)。
pub fn redeem_ticket_existing(
    code: &str,
    user: &auth::User,
    device_name: &str,
    node_id: &str,
) -> Result<String, String> {
    crate::collab::device_label::validate_characters(device_name)?;
    let device_name = device_name.trim();
    let node_id = node_id.trim();
    if device_name.is_empty()
        || device_name.chars().count() > 80
        || device_name.chars().any(char::is_control)
    {
        return Err("设备名称长度须为 1–80 个字符且不能包含控制字符".into());
    }
    if node_id.is_empty() || node_id.len() > 256 || node_id.chars().any(char::is_control) {
        return Err("设备标识无效".into());
    }
    let mut conn = open_db()?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| format!("兑换事务失败: {e}"))?;
    let (expires_at, used_at) = tx
        .query_row(
            "SELECT expires_at,used_at FROM tickets WHERE code=?1",
            params![code.trim()],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?)),
        )
        .map_err(|_| "票据无效".to_string())?;
    if used_at.is_some() {
        return Err("票据已被使用".into());
    }
    if expires_at < now() {
        return Err("票据已过期".into());
    }
    // 原子核销 + 记账号(与新建号路径同一并发语义)。
    let claimed = tx
        .execute(
            "UPDATE tickets SET used_at=?1, used_by=?3 WHERE code=?2 AND used_at IS NULL",
            params![now(), code.trim(), user.username],
        )
        .map_err(|e| e.to_string())?;
    if claimed != 1 {
        return Err("票据已被使用".into());
    }
    // 设备绑定(事务内,防并发抢绑同一 node_id)。
    let existing = tx
        .query_row(
            "SELECT id,user_id FROM devices WHERE node_id=?1",
            params![node_id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other.to_string()),
        })?;
    let device_id = match existing {
        Some((_, owner)) if owner != user.id => {
            return Err("该设备已属其他账号,拒绝顶替".into());
        }
        Some((id, _)) => id,
        None => {
            let id = random_device_id()?;
            tx.execute(
                "INSERT INTO devices(id,user_id,name,node_id,added_at) VALUES(?1,?2,?3,?4,?5)",
                params![id, user.id, device_name, node_id, now()],
            )
            .map_err(|e| format!("登记设备失败: {e}"))?;
            id
        }
    };
    tx.commit().map_err(|e| format!("提交兑换失败: {e}"))?;
    db::audit(
        &user.username,
        "ticket.redeem_existing",
        code.trim(),
        device_name,
    );
    Ok(device_id)
}

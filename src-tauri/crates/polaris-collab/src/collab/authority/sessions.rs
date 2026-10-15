//! Local membership and federated session issuance.
use super::*;

pub fn login_with_assertion(assertion: &str, device_id: &str) -> Result<(User, String), String> {
    login_with_assertion_ex(assertion, device_id, false)
}

/// **本机控制台**版的同一件事:用户就坐在这台机器前,在桌面 App 里亲手填的邮箱验证码。
///
/// 只多一档准入 [`Admission::LocalConsole`](认领已有的那一行 / 认领这台还没主人的机器)。
/// 调用方是 mesh 的本机登录编排(验证码/密码 Tauri 命令)。**不要**把它接到任何 HTTP
/// 路由上:那就等于把「远程夺主」做成一条 API。
pub fn login_with_assertion_local(
    assertion: &str,
    device_id: &str,
) -> Result<(User, String), String> {
    login_with_assertion_ex(assertion, device_id, true)
}

fn login_with_assertion_ex(
    assertion: &str,
    device_id: &str,
    local_console: bool,
) -> Result<(User, String), String> {
    let claims = verify_assertion(assertion)?;
    let dev = super::super::interconnect::binding::node(&claims, device_id)?;
    if !dev.is_empty() && super::super::grants::is_revoked(dev) {
        db::audit(
            &claims.username,
            "auth.login.denied",
            dev,
            "设备已被本机撤销",
        );
        return Err(
            "这台设备已被移出本机的信任列表 —— 请在另一台设备的「设备台账」里恢复它".into(),
        );
    }
    let owner = owner_uid();
    let is_mine = !owner.is_empty() && owner == claims.uid;
    let admission = if is_mine {
        Admission::SelfOwned
    } else if local_console {
        Admission::LocalConsole
    } else {
        Admission::ExistingOnly
    };
    let user = upsert_member(&claims, admission)?;

    // Re-read ownership after local-console bootstrap; do not overwrite explicit contracts.
    if owner_uid() == claims.uid && !dev.is_empty() {
        match super::super::grants::auto_grant_self(dev, &claims.uid, &user.display_name, false) {
            Ok((_, true)) => db::audit(&user.username, "auth.login.newdevice", dev, "策略 B：新设备全权接入"),
            Ok(_) => {}
            Err(error) => return Err(format!("登记设备契约失败: {error}")),
        }
    }

    let token = super::super::interconnect::binding::issue(user.id, &claims, dev)?;
    db::audit(&user.username, "auth.login.federated", dev, &claims.uid);
    Ok((user, token))
}

/// 云端账号 + 一次性邀请票据 → 成为本机成员并拿到本机会话。
///
/// 票据的原子核销与成员资格的创建**必须**在同一事务里,否则并发两个人拿同一张票据
/// 都能进来。这里的做法与 identity::redeem_ticket 一致:先 CAS 占用票据(`WHERE
/// used_at IS NULL`),占不到就是被人抢先了,直接拒。
pub fn join_with_ticket(
    assertion: &str,
    code: &str,
    device_name: &str,
    node_id: &str,
) -> Result<(User, String), String> {
    let claims = verify_assertion(assertion)?;
    let node_id = super::super::interconnect::binding::node(&claims, node_id)?;
    // 已经是成员(或本机主人自己)就不必再烧票据,直接当登录处理 ——
    // 重复点「入伙」不该白白作废一张票,主人对自己的机器更不该被要求出示邀请码。
    let owner = owner_uid();
    let quick = if !owner.is_empty() && owner == claims.uid {
        Admission::SelfOwned
    } else {
        Admission::ExistingOnly
    };
    if let Ok(u) = upsert_member(&claims, quick) {
        let token = super::super::interconnect::binding::issue(u.id, &claims, node_id)?;
        return Ok((u, token));
    }
    let code = normalize_invite(code);
    if code.is_empty() {
        return Err("请填写这台机器的设备码".into());
    }
    // **一码**:本机那串常驻设备码在这里也认。
    //
    // 在此之前「进这台机器的门」有两串完全不同的码:一次性邀请票据(账号入伙用)和
    // PLRK1 连接码(设备连接用),它们的区别纯粹是我们自己的内部分工,用户只看到
    // 「怎么又是另一种码」。现在设备码一串通吃:拿它入伙,拿到的是 **visitor** ——
    // 与 `/api/collab/connect` 那条路同一个档,能做什么仍由设备契约逐台说了算。
    let role = if super::super::identity::check_access_code(&code).unwrap_or(false) {
        db::audit(&claims.username, "join.devicecode", node_id, "设备码入伙");
        "visitor".to_string()
    } else {
        burn_ticket(&code, &claims.username)?
    };
    let user = upsert_member(&claims, Admission::Invited(role.clone()))?;
    // 设备白名单:入伙即登记本设备(隧道层准入的那一因子)。失败不阻断入伙。
    let nid = node_id.trim();
    if !nid.is_empty() {
        let name = if device_name.trim().is_empty() {
            "新设备"
        } else {
            device_name.trim()
        };
        let _ = super::super::identity::add_device(user.id, name, nid);
    }
    let token = super::super::interconnect::binding::issue(user.id, &claims, nid)?;
    db::audit(&claims.username, "ticket.redeem.federated", &code, &role);
    Ok((user, token))
}


//! collab/grants.rs —— 设备信任契约(`peer_grants`)。
//!
//! 解决的是「记住权限的状态」这件事。此前的实际行为是:每次重连都重新握手、重新问一遍
//! 能不能挂盘,而挂载配置只活在 fsmount 的内存态里 —— 重启一次全忘光,用户要重新点一遍。
//! 这张表记的是**用户的意图**(我批准了这台设备、批到什么档、要不要自动挂盘),不是运行态,
//! 所以它跨重启、跨换网、跨换 IP 都成立。
//!
//! 一行契约同时服务两个方向 —— 它描述的是「我和这台设备之间的信任」,不是单向配置:
//!  · **出方向**:后台对账循环据此决定连不连它、挂不挂盘、挂只读还是可读写。
//!  · **入方向**:它拿断言来进本机门时,`revoked=1` 直接拒 —— 这是云端目录之外的第二道闸,
//!    云机被绕过、名册被伪造,本机这一行仍然说了算。
//!
//! 策略 B：同账号新设备立即按契约全权；高风险操作由 interconnect::gate 单独批准。
use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use super::db::{self, now, open_db};

/// 盘访问档。
pub const FS_NONE: &str = "none";
pub const FS_RO: &str = "ro";
pub const FS_RW: &str = "rw";

/// 远程执行档。
pub const EXEC_NONE: &str = "none";
pub const EXEC_ASK: &str = "ask";
pub const EXEC_ALLOW: &str = "allow";

mod contract;
pub use contract::Grant;

fn norm_fs(v: &str) -> Result<&'static str, String> {
    match v.trim() {
        FS_NONE => Ok(FS_NONE),
        FS_RO => Ok(FS_RO),
        FS_RW => Ok(FS_RW),
        other => Err(format!("盘访问档只能是 none/ro/rw,收到「{other}」")),
    }
}

fn norm_exec(v: &str) -> Result<&'static str, String> {
    match v.trim() {
        EXEC_NONE => Ok(EXEC_NONE),
        EXEC_ASK => Ok(EXEC_ASK),
        EXEC_ALLOW => Ok(EXEC_ALLOW),
        other => Err(format!("执行档只能是 none/ask/allow,收到「{other}」")),
    }
}

fn row_to_grant(r: &rusqlite::Row) -> rusqlite::Result<Grant> {
    Ok(Grant {
        node_id: r.get(0)?,
        uid: r.get(1)?,
        name: r.get(2)?,
        role: r.get(3)?,
        fs_access: r.get(4)?,
        exec_access: r.get(5)?,
        auto_mount: r.get::<_, i64>(6)? != 0,
        drive_hint: r.get(7)?,
        cooldown_until: r.get(8)?,
        granted_at: r.get(9)?,
        granted_by: r.get(10)?,
        revoked: r.get::<_, i64>(11)? != 0,
    })
}

const COLS: &str = "node_id,uid,name,role,fs_access,exec_access,auto_mount,drive_hint,\
                    cooldown_until,granted_at,granted_by,revoked";

/// 查一台设备的契约。没批过 = None。
pub fn get(node_id: &str) -> Result<Option<Grant>, String> {
    let node_id = node_id.trim();
    if node_id.is_empty() {
        return Ok(None);
    }
    let conn = open_db()?;
    conn.query_row(
        &format!("SELECT {COLS} FROM peer_grants WHERE node_id=?1"),
        params![node_id],
        row_to_grant,
    )
    .optional()
    .map_err(|e| format!("查设备契约失败: {e}"))
}

/// 全部契约(含已撤销的 —— 台账要能看见「这台被我踢过」)。
pub fn list() -> Result<Vec<Grant>, String> {
    let conn = open_db()?;
    let mut st = conn
        .prepare(&format!(
            "SELECT {COLS} FROM peer_grants ORDER BY revoked ASC, granted_at DESC"
        ))
        .map_err(|e| format!("查设备契约失败: {e}"))?;
    let rows = st
        .query_map([], row_to_grant)
        .map_err(|e| format!("查设备契约失败: {e}"))?;
    Ok(rows.flatten().collect())
}

/// 首次见到一台**自己账号的**设备时自动批准。
///
/// 返回 (契约, 是否新建)。已存在就原样返回 —— 用户后来手工调过的档位,不能被一次重连覆盖回默认值,
/// 那等于「每次重启都把权限重置一遍」,正是这次要修掉的毛病。
///
/// `is_first_device` 参数保留调用兼容；策略 B 的所有新设备都立即按全权契约接入。
mod automatic;
pub use automatic::auto_grant_self;

/// 用户在设备台账上改档位。只改传进来的字段(None = 不动)。
#[allow(clippy::too_many_arguments)]
pub fn update(
    node_id: &str,
    name: Option<&str>,
    fs_access: Option<&str>,
    exec_access: Option<&str>,
    auto_mount: Option<bool>,
    actor: &str,
) -> Result<Grant, String> {
    if let Some(name) = name { super::device_label::validate_characters(name)?; }
    let node_id = node_id.trim();
    let cur = get(node_id)?.ok_or("这台设备还没有信任契约")?;
    let fs = match fs_access {
        Some(v) => norm_fs(v)?.to_string(),
        None => cur.fs_access.clone(),
    };
    let ex = match exec_access {
        Some(v) => norm_exec(v)?.to_string(),
        None => cur.exec_access.clone(),
    };
    let nm = name
        .map(|s| s.trim().to_string())
        .unwrap_or(cur.name.clone());
    let am = auto_mount.unwrap_or(cur.auto_mount);
    let conn = open_db()?;
    conn.execute(
        "UPDATE peer_grants SET name=?2, fs_access=?3, exec_access=?4, auto_mount=?5, \
         granted_by=?6 WHERE node_id=?1",
        params![node_id, nm, fs, ex, i64::from(am), actor],
    )
    .map_err(|e| format!("改设备契约失败: {e}"))?;
    db::audit(
        actor,
        "peer.grant.update",
        node_id,
        &format!("fs={fs} exec={ex} auto={am}"),
    );
    get(node_id)?.ok_or_else(|| "改完读不回来".into())
}

/// 撤销一台设备的准入(丢了电脑 / 收回授权)。
/// 幂等:没批过也当成功 —— 用户的意图是「这台不许进」,而它本来就进不来。
pub fn revoke(node_id: &str, actor: &str) -> Result<(), String> {
    let node_id = node_id.trim();
    if node_id.is_empty() {
        return Err("缺 NodeId".into());
    }
    let conn = open_db()?;
    conn.execute(
        "INSERT INTO peer_grants(node_id,granted_at,revoked) VALUES(?1,?2,1) \
         ON CONFLICT(node_id) DO UPDATE SET revoked=1",
        params![node_id, now()],
    )
    .map_err(|e| format!("撤销设备失败: {e}"))?;
    db::audit(actor, "peer.grant.revoke", node_id, "");
    Ok(())
}

/// 记住这台设备上次挂成了哪个盘符(下次尽量复原,免得每次重启换个字母,
/// 用户存的快捷方式、脚本里的路径全断)。
pub fn remember_drive(node_id: &str, drive: &str) -> Result<(), String> {
    let conn = open_db()?;
    conn.execute(
        "UPDATE peer_grants SET drive_hint=?2 WHERE node_id=?1",
        params![node_id.trim(), drive.trim()],
    )
    .map_err(|e| format!("记盘符失败: {e}"))?;
    Ok(())
}

/// 本机此前批过设备没有(判「是不是第一台」用)。
pub fn any_granted() -> Result<bool, String> {
    let conn = open_db()?;
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM peer_grants WHERE revoked=0",
            [],
            |r| r.get(0),
        )
        .map_err(|e| format!("查设备契约失败: {e}"))?;
    Ok(n > 0)
}

/// 入方向的闸:这台设备被撤销过吗。
/// **只认已撤销**,不认「没有契约」—— 没契约的设备走的是别的门(邀请码/首登建 owner),
/// 那些路各有各的判定,这里越权拒绝会把正常的首次登录也堵死。
pub fn is_revoked(node_id: &str) -> bool {
    matches!(get(node_id), Ok(Some(g)) if g.revoked)
}

// ───────────────────────── 入方向:按设备放行 ─────────────────────────
//
// 在此之前这张表**只在出方向消费**(本机挂对端的盘时定 ro/rw),入方向只判了一个
// `revoked`。也就是说台账上那两排「盘:只读/读写」「远程执行:禁止/每次问/允许」
// 对**连进来的人**根本不生效 —— 进得来就是 owner 全权。设备码这条路一旦开放,
// 那等于「谁拿到码谁就能删你的盘」。下面两道闸把台账上的档位变成真的。
//
// 判定顺序刻意如此:
//  ① 没有 device_id(全局口令 admin / 老会话 / 本机内部调用)→ 放行,交给原来的角色闸。
//     这条是为了**不破坏现状**:桌面自己、owner 令牌、CI 探针走的都是这条。
//  ② device_id 就是本机自己 → 放行。桌面登录时会话上记的是本机 NodeId,而本机
//     也给自己建了一行契约;本机会话仍走角色闸。
//  ③ 有契约 → 契约说了算(显式权限/撤销都已经折进 effective_*)。
//  ④ 有 device_id 但没契约 → 放行,交给角色闸。没契约的是「还没走过设备码那条路」的
//     老客户端(手机壳粘 PLRK1 之类),它们本来就要过 owner 闸,这里不额外收紧。

/// 本机自己那台设备的 NodeId。空 = 隧道还没起过(那时也不会有对端进来)。
fn my_node() -> String {
    db::meta_get("host_node_id").unwrap_or_default()
}

/// 这个 device_id 是不是「不受契约约束」的自己人(见上面的 ①②④)。
fn exempt(device_id: &str) -> bool {
    let d = device_id.trim();
    if d.is_empty() {
        return true;
    }
    let me = my_node();
    !me.is_empty() && me == d
}

/// **盘闸**:这台设备能不能读 / 能不能写本机共享出去的目录。
/// 拒绝时回的是人话 —— 它会一路透到对方的资源管理器里当失败原因显示。
pub fn fs_gate(device_id: &str, need_write: bool) -> Result<(), String> {
    if exempt(device_id) {
        return Ok(());
    }
    let Some(g) = get(device_id).unwrap_or(None) else {
        return Ok(());
    };
    match g.effective_fs() {
        FS_RW => Ok(()),
        FS_RO if !need_write => Ok(()),
        FS_RO => Err("对方把这台设备设成了只读 —— 请他在「设备与授权」里把盘改成「可读写」".into()),
        _ => Err(if g.revoked {
            "这台设备已被对方移出信任列表".into()
        } else {
            "对方没有把盘开放给这台设备(当前档位:不挂盘)".into()
        }),
    }
}

/// **执行闸**:这台设备能不能在本机跑命令。`ask` 当前按拒处理 ——
/// 「每次问」需要一条弹窗确认通道,那条通道还没有;在它到位之前宁可拒,
/// 也不能把一个中间档静默当成「允许」。
pub fn exec_gate(device_id: &str) -> Result<(), String> {
    if exempt(device_id) {
        return Ok(());
    }
    let Some(g) = get(device_id).unwrap_or(None) else {
        return Ok(());
    };
    match g.effective_exec() {
        EXEC_ALLOW => Ok(()),
        EXEC_ASK => Err(
            "对方把这台设备的远程执行设成了「每次问」,而本机还没有弹窗确认通道 —— 请他改成「允许」"
                .into(),
        ),
        _ => Err("对方没有允许这台设备远程执行(当前档位:禁止)".into()),
    }
}

/// 拿**设备码**连进来的对端:默认只读、禁执行、不自动挂盘。
///
/// 与 [`auto_grant_self`] 的区别正是「这是别人」——后者是同一个账号的自己人,默认全权;
/// 这里默认最小权限,由本机主人在台账上点「信任这台」再升档。已有契约原样沿用
/// (用户调过的档不能被对方重连一次冲回默认值),已撤销的直接拒。
pub fn grant_by_code(node_id: &str, name: &str) -> Result<Grant, String> {
    super::device_label::validate_characters(name)?;
    let node_id = node_id.trim();
    if node_id.is_empty() {
        return Err("缺 NodeId".into());
    }
    if let Some(g) = get(node_id)? {
        if g.revoked {
            return Err(
                "这台设备已被移出本机的信任列表 —— 请机主先在「设备与授权」里恢复它".into(),
            );
        }
        return Ok(g);
    }
    let t = now();
    let conn = open_db()?;
    conn.execute(
        "INSERT INTO peer_grants(node_id,uid,name,role,fs_access,exec_access,auto_mount,\
         drive_hint,cooldown_until,granted_at,granted_by,revoked) \
         VALUES(?1,'',?2,'visitor',?3,?4,0,'',0,?5,?6,0)",
        params![node_id, name.trim(), FS_RO, EXEC_NONE, t, BY_CODE],
    )
    .map_err(|e| format!("登记设备契约失败: {e}"))?;
    db::audit("system", "peer.grant.code", node_id, "设备码接入·默认只读");
    get(node_id)?.ok_or_else(|| "刚写入的契约读不回来".into())
}

/// `granted_by` 上标记「这一行是拿设备码进来的」。换码时要按它把旧会话清干净。
pub const BY_CODE: &str = "code";

/// 「信任这台设备」：恢复被撤销的准入并将显式权限升为全权。
pub fn trust_fully(node_id: &str, actor: &str) -> Result<Grant, String> {
    let node_id = node_id.trim();
    if get(node_id)?.is_none() {
        return Err("这台设备还没有信任契约".into());
    }
    let conn = open_db()?;
    // `granted_by` 同时记着准入来源。靠设备码进来的行必须一直保留 `code`，否则机主
    // 点一次「信任这台」后再换码，rotate_access_code 就找不到并吊销它的旧会话。
    conn.execute(
        "UPDATE peer_grants SET cooldown_until=0, revoked=0, fs_access=?2, exec_access=?3, \
         auto_mount=1 WHERE node_id=?1",
        params![node_id, FS_RW, EXEC_ALLOW],
    )
    .map_err(|e| format!("信任设备失败: {e}"))?;
    db::audit(actor, "peer.grant.trust", node_id, "已升为全权");
    get(node_id)?.ok_or_else(|| "改完读不回来".into())
}

#[cfg(test)]
mod gate_tests;

#[cfg(test)]
mod tests;

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
//! ## 冷静期(cooldown)
//!
//! `SelfOwned` 准入让「同一个账号的设备免邀请码」,代价是:云机若被拿下,伪造一张断言就能
//! 直接成为你每台机器的 owner。冷静期是这一让步的对冲 —— 一台**从没见过的**新设备首次进来,
//! 24 小时内一律按只读对待,且这段时间里用户能一键撤销。把「静默拿到全权」压成
//! 「24 小时内可反悔的只读」。用户在设备台账上点「信任这台设备」即当场清零。
//!
//! 注意冷静期只对**账号里已经有别的设备**时生效:第一台设备就是你本人在装机,拦它没有意义,
//! 只会让「填个邮箱就能用」变成「填完还要等一天」。
use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use super::db::{self, now, open_db};

/// 新设备冷静期时长(秒)。
pub const COOLDOWN_SECS: i64 = 24 * 3600;

/// 盘访问档。
pub const FS_NONE: &str = "none";
pub const FS_RO: &str = "ro";
pub const FS_RW: &str = "rw";

/// 远程执行档。
pub const EXEC_NONE: &str = "none";
pub const EXEC_ASK: &str = "ask";
pub const EXEC_ALLOW: &str = "allow";

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Grant {
    pub node_id: String,
    pub uid: String,
    pub name: String,
    pub role: String,
    /// 库里存的档位(用户批的那个)。实际生效的档见 [`Grant::effective_fs`]。
    pub fs_access: String,
    pub exec_access: String,
    pub auto_mount: bool,
    pub drive_hint: String,
    pub cooldown_until: i64,
    pub granted_at: i64,
    pub granted_by: String,
    pub revoked: bool,
}

impl Grant {
    /// 冷静期内还在观察的新设备:UI 要显著提示,用户据此决定信任还是撤销。
    pub fn in_cooldown(&self) -> bool {
        self.cooldown_until > now()
    }

    /// **实际**生效的盘访问档。冷静期内一律降到只读 —— 这就是那句「可反悔的只读」
    /// 落在代码里的样子:库里记着用户批的 rw,但这 24 小时内谁问都只给 ro。
    pub fn effective_fs(&self) -> &str {
        if self.revoked {
            return FS_NONE;
        }
        if self.in_cooldown() && self.fs_access == FS_RW {
            return FS_RO;
        }
        &self.fs_access
    }

    /// 实际生效的执行档。冷静期内不给 allow(降到 ask,由人当场确认)。
    pub fn effective_exec(&self) -> &str {
        if self.revoked {
            return EXEC_NONE;
        }
        if self.in_cooldown() && self.exec_access == EXEC_ALLOW {
            return EXEC_ASK;
        }
        &self.exec_access
    }

    /// 这台设备现在该不该被自动连上并挂盘。
    pub fn should_mount(&self) -> bool {
        !self.revoked && self.auto_mount && self.effective_fs() != FS_NONE
    }
}

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
/// `is_first_device=true`(本机此前一台设备都没批过)时不设冷静期:那就是你本人在装第一台机器,
/// 拦它只会把「填个邮箱就能用」变成「填完还要等一天」。
pub fn auto_grant_self(
    node_id: &str,
    uid: &str,
    name: &str,
    is_first_device: bool,
) -> Result<(Grant, bool), String> {
    let node_id = node_id.trim();
    if node_id.is_empty() {
        return Err("缺 NodeId".into());
    }
    if let Some(g) = get(node_id)? {
        return Ok((g, false));
    }
    let t = now();
    let cooldown = if is_first_device {
        0
    } else {
        t + COOLDOWN_SECS
    };
    let conn = open_db()?;
    conn.execute(
        "INSERT INTO peer_grants(node_id,uid,name,role,fs_access,exec_access,auto_mount,\
         drive_hint,cooldown_until,granted_at,granted_by,revoked) \
         VALUES(?1,?2,?3,'owner',?4,?5,1,'',?6,?7,'auto:same-account',0)",
        params![
            node_id,
            uid.trim(),
            name.trim(),
            FS_RW,
            EXEC_ALLOW,
            cooldown,
            t
        ],
    )
    .map_err(|e| format!("登记设备契约失败: {e}"))?;
    db::audit(
        uid,
        "peer.grant.auto",
        node_id,
        if cooldown > 0 {
            "同账号设备·冷静期中"
        } else {
            "同账号设备·首台"
        },
    );
    Ok((get(node_id)?.ok_or("刚写入的契约读不回来")?, true))
}

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

/// 用户点「信任这台设备」:冷静期立即清零。
pub fn trust_now(node_id: &str, actor: &str) -> Result<Grant, String> {
    let node_id = node_id.trim();
    let conn = open_db()?;
    let n = conn
        .execute(
            "UPDATE peer_grants SET cooldown_until=0, revoked=0 WHERE node_id=?1",
            params![node_id],
        )
        .map_err(|e| format!("信任设备失败: {e}"))?;
    if n == 0 {
        return Err("这台设备还没有信任契约".into());
    }
    db::audit(actor, "peer.grant.trust", node_id, "冷静期已清零");
    get(node_id)?.ok_or_else(|| "改完读不回来".into())
}

/// 撤销一台设备的准入(丢了电脑 / 冷静期内反悔)。
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
//     也给自己建了一行契约;要是让它受契约约束,本机排在「第二台设备」时会被自己的
//     24 小时冷静期降成只读 —— 自己把自己锁了。
//  ③ 有契约 → 契约说了算(冷静期/撤销都已经折进 effective_*)。
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
        FS_RO => Err(if g.in_cooldown() {
            "这台设备还在新设备观察期内,暂时只读 —— 在对方机器的「设备与授权」里点「信任这台」即可写"
                .into()
        } else {
            "对方把这台设备设成了只读 —— 请他在「设备与授权」里把盘改成「可读写」".into()
        }),
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
        EXEC_ASK if g.in_cooldown() => {
            Err("这台设备还在新设备观察期内,暂不允许远程执行 —— 对方点一下「信任这台」即可".into())
        }
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

/// 「信任这台设备」的完整含义:冷静期清零 + 升到全权。
///
/// 为什么不复用 [`trust_now`]:那个只清冷静期,对**设备码**进来的设备毫无作用 ——
/// 它压根没有冷静期,它的限制是 `fs=ro/exec=none` 这两个档位本身。用户点的那颗按钮
/// 心里想的是「这台我认了,放开」,所以两件事必须一起做。
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
mod gate_tests {
    use super::*;

    fn tmp(tag: &str) -> std::sync::MutexGuard<'static, ()> {
        let g = db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var(
            "POLARIS_COLLAB_DB",
            std::env::temp_dir().join(format!(
                "grants-gate-{tag}-{}-{}.db",
                std::process::id(),
                now()
            )),
        );
        g
    }

    /// 设备码进来的对端:**默认只读、禁执行**。这是用户拍板的那一档。
    #[test]
    fn code_peer_is_read_only_until_trusted() {
        let _g = tmp("code");
        let peer = "node-someone-else";
        let created = grant_by_code(peer, "小李的笔记本").unwrap();
        assert_eq!(created.fs_access, FS_RO);
        assert_eq!(created.exec_access, EXEC_NONE);
        assert!(!created.auto_mount, "别人的设备不该自动挂进我的盘符");

        assert!(fs_gate(peer, false).is_ok(), "读要放行");
        let e = fs_gate(peer, true).unwrap_err();
        assert!(e.contains("只读"), "写必须被挡住,err={e}");
        assert!(exec_gate(peer).unwrap_err().contains("禁止"));

        // 机主点「信任这台」→ 一步升到全权。
        trust_fully(peer, "本机").unwrap();
        assert!(fs_gate(peer, true).is_ok());
        assert!(exec_gate(peer).is_ok());
    }

    /// 重连不能把机主调过的档冲回默认值 —— 那等于「每次重连权限重置一遍」。
    #[test]
    fn reconnect_keeps_the_owner_decision() {
        let _g = tmp("keep");
        grant_by_code("node-p", "对方").unwrap();
        trust_fully("node-p", "本机").unwrap();
        let again = grant_by_code("node-p", "对方").unwrap();
        assert_eq!(again.fs_access, FS_RW, "重连不该把 rw 冲回 ro");
        // 反过来也一样:降成不挂盘之后,对方重连仍是不挂盘。
        update("node-p", None, Some(FS_NONE), None, None, "本机").unwrap();
        assert_eq!(grant_by_code("node-p", "对方").unwrap().fs_access, FS_NONE);
        assert!(fs_gate("node-p", false).unwrap_err().contains("不挂盘"));
    }

    /// 踢掉的设备拿着旧码回来也进不去。
    #[test]
    fn revoked_peer_cannot_reconnect_with_the_code() {
        let _g = tmp("revoked");
        grant_by_code("node-bad", "谁").unwrap();
        revoke("node-bad", "本机").unwrap();
        assert!(grant_by_code("node-bad", "谁")
            .unwrap_err()
            .contains("移出"));
        assert!(fs_gate("node-bad", false).is_err());
        assert!(exec_gate("node-bad").is_err());
    }

    /// 本机自己不受契约约束:桌面会话上记的是本机 NodeId,而本机也给自己建了一行契约。
    /// 要是让它受约束,本机排在「第二台设备」时会被自己的 24 小时冷静期降成只读 —— 自锁。
    #[test]
    fn my_own_machine_is_never_gated_by_its_own_grant() {
        let _g = tmp("self");
        db::meta_set("host_node_id", "node-me").unwrap();
        // 造一行最严的契约扣在自己头上。
        grant_by_code("node-me", "我自己").unwrap();
        update(
            "node-me",
            None,
            Some(FS_NONE),
            Some(EXEC_NONE),
            None,
            "本机",
        )
        .unwrap();
        assert!(fs_gate("node-me", true).is_ok(), "本机不该被自己的契约锁住");
        assert!(exec_gate("node-me").is_ok());
        // 空 device_id(全局口令 / 老会话 / 本机内部调用)同样豁免。
        assert!(fs_gate("", true).is_ok());
        assert!(exec_gate("").is_ok());
    }

    /// 没有契约的设备不额外收紧 —— 老手机壳粘 PLRK1 那条路仍旧只过角色闸。
    #[test]
    fn unknown_device_falls_back_to_the_role_gate() {
        let _g = tmp("unknown");
        db::meta_set("host_node_id", "node-me").unwrap();
        assert!(fs_gate("node-never-seen", true).is_ok());
        assert!(exec_gate("node-never-seen").is_ok());
    }

    /// 同账号设备的冷静期在**入方向**也真的生效了(此前只影响本机挂对端时的 ro/rw)。
    #[test]
    fn same_account_cooldown_now_bites_on_the_way_in() {
        let _g = tmp("cooldown");
        db::meta_set("host_node_id", "node-me").unwrap();
        auto_grant_self("node-first", "acct_1", "第一台", true).unwrap();
        let (second, _) = auto_grant_self("node-2nd", "acct_1", "新来的", false).unwrap();
        assert!(second.in_cooldown());
        assert!(fs_gate("node-2nd", false).is_ok(), "读照旧");
        assert!(
            fs_gate("node-2nd", true).unwrap_err().contains("观察期"),
            "冷静期内写要被挡"
        );
        assert!(exec_gate("node-2nd").unwrap_err().contains("观察期"));
        // 第一台没有冷静期,照旧全权。
        assert!(fs_gate("node-first", true).is_ok());
        assert!(exec_gate("node-first").is_ok());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_db(tag: &str) -> std::sync::MutexGuard<'static, ()> {
        let g = db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let p = std::env::temp_dir().join(format!("grants-test-{}-{tag}.db", std::process::id()));
        let _ = std::fs::remove_file(&p);
        std::env::set_var("POLARIS_COLLAB_DB", &p);
        g
    }

    /// 第一台设备免冷静期直接全权;第二台起进冷静期,期间**实际**只给只读。
    #[test]
    fn first_device_is_full_second_is_cooled() {
        let _g = tmp_db("cooldown");
        let (first, fresh) = auto_grant_self("node-a", "acct_1", "我的电脑", true).unwrap();
        assert!(fresh);
        assert!(!first.in_cooldown(), "第一台设备不该有冷静期");
        assert_eq!(first.effective_fs(), FS_RW);
        assert_eq!(first.effective_exec(), EXEC_ALLOW);
        assert!(first.should_mount());

        let (second, _) = auto_grant_self("node-b", "acct_1", "NAS", false).unwrap();
        assert!(second.in_cooldown(), "第二台起必须进冷静期");
        assert_eq!(second.fs_access, FS_RW, "库里记的仍是用户批的档");
        assert_eq!(second.effective_fs(), FS_RO, "但这 24 小时内实际只给只读");
        assert_eq!(second.effective_exec(), EXEC_ASK);
        assert!(second.should_mount(), "只读盘照样该挂上,只是不给写");
    }

    /// 重连不该把用户手工调过的档位冲回默认值 —— 「每次重启权限重置一遍」正是要修的毛病。
    #[test]
    fn reconnect_does_not_reset_user_choice() {
        let _g = tmp_db("keep");
        auto_grant_self("node-a", "acct_1", "NAS", true).unwrap();
        update(
            "node-a",
            None,
            Some(FS_RO),
            Some(EXEC_NONE),
            Some(false),
            "wuli",
        )
        .unwrap();

        let (again, fresh) = auto_grant_self("node-a", "acct_1", "NAS", true).unwrap();
        assert!(!fresh, "已有契约不该被当成新设备");
        assert_eq!(again.fs_access, FS_RO);
        assert_eq!(again.exec_access, EXEC_NONE);
        assert!(!again.auto_mount);
        assert!(!again.should_mount(), "用户关掉自动挂载后就不该再自动挂");
    }

    /// 「信任」按钮:冷静期当场清零,全权立刻生效。
    #[test]
    fn trust_clears_cooldown() {
        let _g = tmp_db("trust");
        auto_grant_self("node-a", "acct_1", "第一台", true).unwrap();
        let (g, _) = auto_grant_self("node-b", "acct_1", "新设备", false).unwrap();
        assert_eq!(g.effective_fs(), FS_RO);

        let g = trust_now("node-b", "wuli").unwrap();
        assert!(!g.in_cooldown());
        assert_eq!(g.effective_fs(), FS_RW);
    }

    /// 撤销 = 入方向直接拒 + 出方向不再挂。没批过的设备不算被撤销(它走别的门)。
    #[test]
    fn revoke_denies_both_directions() {
        let _g = tmp_db("revoke");
        auto_grant_self("node-a", "acct_1", "旧电脑", true).unwrap();
        assert!(!is_revoked("node-a"));

        revoke("node-a", "wuli").unwrap();
        assert!(is_revoked("node-a"));
        let g = get("node-a").unwrap().unwrap();
        assert_eq!(g.effective_fs(), FS_NONE);
        assert_eq!(g.effective_exec(), EXEC_NONE);
        assert!(!g.should_mount());

        // 没批过的设备:不是「被撤销」,别越权把正常首登堵死。
        assert!(!is_revoked("node-never-seen"));
        // 撤销没见过的设备也得成功(丢电脑时用户不该被「它还没登记」挡住)。
        revoke("node-ghost", "wuli").unwrap();
        assert!(is_revoked("node-ghost"));
    }

    #[test]
    fn bad_levels_are_refused() {
        let _g = tmp_db("bad");
        auto_grant_self("node-a", "acct_1", "机器", true).unwrap();
        assert!(update("node-a", None, Some("everything"), None, None, "x").is_err());
        assert!(update("node-a", None, None, Some("root"), None, "x").is_err());
        // 改不存在的设备要报错,不能静默建一条出来。
        assert!(update("node-zzz", None, Some(FS_RO), None, None, "x").is_err());
    }

    #[test]
    fn drive_hint_survives() {
        let _g = tmp_db("drive");
        auto_grant_self("node-a", "acct_1", "NAS", true).unwrap();
        remember_drive("node-a", "Y:").unwrap();
        assert_eq!(get("node-a").unwrap().unwrap().drive_hint, "Y:");
        assert!(any_granted().unwrap());
    }
}

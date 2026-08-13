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
    let cooldown = if is_first_device { 0 } else { t + COOLDOWN_SECS };
    let conn = open_db()?;
    conn.execute(
        "INSERT INTO peer_grants(node_id,uid,name,role,fs_access,exec_access,auto_mount,\
         drive_hint,cooldown_until,granted_at,granted_by,revoked) \
         VALUES(?1,?2,?3,'owner',?4,?5,1,'',?6,?7,'auto:same-account',0)",
        params![node_id, uid.trim(), name.trim(), FS_RW, EXEC_ALLOW, cooldown, t],
    )
    .map_err(|e| format!("登记设备契约失败: {e}"))?;
    db::audit(
        uid,
        "peer.grant.auto",
        node_id,
        if cooldown > 0 { "同账号设备·冷静期中" } else { "同账号设备·首台" },
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
    let nm = name.map(|s| s.trim().to_string()).unwrap_or(cur.name.clone());
    let am = auto_mount.unwrap_or(cur.auto_mount);
    let conn = open_db()?;
    conn.execute(
        "UPDATE peer_grants SET name=?2, fs_access=?3, exec_access=?4, auto_mount=?5, \
         granted_by=?6 WHERE node_id=?1",
        params![node_id, nm, fs, ex, i64::from(am), actor],
    )
    .map_err(|e| format!("改设备契约失败: {e}"))?;
    db::audit(actor, "peer.grant.update", node_id, &format!("fs={fs} exec={ex} auto={am}"));
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
        .query_row("SELECT COUNT(*) FROM peer_grants WHERE revoked=0", [], |r| r.get(0))
        .map_err(|e| format!("查设备契约失败: {e}"))?;
    Ok(n > 0)
}

/// 入方向的闸:这台设备被撤销过吗。
/// **只认已撤销**,不认「没有契约」—— 没契约的设备走的是别的门(邀请码/首登建 owner),
/// 那些路各有各的判定,这里越权拒绝会把正常的首次登录也堵死。
pub fn is_revoked(node_id: &str) -> bool {
    matches!(get(node_id), Ok(Some(g)) if g.revoked)
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
        update("node-a", None, Some(FS_RO), Some(EXEC_NONE), Some(false), "wuli").unwrap();

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

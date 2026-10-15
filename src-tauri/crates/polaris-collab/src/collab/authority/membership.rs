//! Local admission and ownership rules.
use super::*;

/// 入伙资格:一张有效断言只证明「你是谁」,**不证明「这台主机欢迎你」**。
/// 两者必须分开 —— 否则任何人在云端注册个账号,就能登进别人家的主机。
/// 而主机上的成员能建项目、push 仓库、触发 checks 跑构建脚本 = 主机 RCE,
/// 这正是 collab_signup 默认关闭要防的事,联邦登录不能把这道门重新打开。
pub enum Admission {
    /// 日常登录:只认已经是本机成员的账号;不是就拒(唯一例外见 upsert_member 里的空库分支)。
    ExistingOnly,
    /// 拿着 owner 签发的一次性邀请票据来入伙,角色由票据决定。
    Invited(String),
    /// **自己名下的设备**:断言里的 uid 与本机记着的 `owner_uid` 一致 → 直接给 owner。
    ///
    /// 为什么开这个口:上面那条铁律(「云端证明你是谁,进不进得去由每台机器说了算」)对
    /// **别人**成立,对**你自己名下的第二台机器**却是纯摩擦 —— 两台机器的主人是同一个 uid,
    /// 却要自己给自己发一张邀请码。用户看到的就是「怎么还要登第二个账号」。
    ///
    /// 代价与对冲说清楚:云机若被完全拿下,它能签任意断言,于是能直接成为你每台机器的 owner
    /// (此前还隔着一道邀请码)。策略 B 允许新设备立即全权；高风险操作另走
    /// interconnect::gate 的设备批准与吊销新鲜度检查。另有 `owner_uid` 只写一次的闸
    /// (见 [`super::db::meta_set_once`]),否则「谁最后登录谁是主人」会把远程夺权做成一条 API。
    SelfOwned,
    /// **本机控制台**:用户就坐在这台机器前,在桌面 App 里亲手填了邮箱和验证码
    /// (`account_login_code`,只有 Tauri 命令这一条路能到,不经 HTTP、不在手机数据面白名单里)。
    ///
    /// 为什么必须有这一档:`owner_uid` 只由**联邦登录**写下。v2.9.0 之前装的机器,主人是
    /// 「设为主机」时建的本地账号(没有 uid、常常也没绑邮箱),于是 `owner_uid` 恒空、
    /// `is_mine` 恒 false —— 主人在自己的电脑上用自己的邮箱登录,反被自己的机器回一句
    /// 「本主机还没有邀请你」。两台老机器互相如此,就成了「云端名册看得见对方,却谁也挂不上谁的盘」。
    ///
    /// 这一档做两件事(见 [`adopt_row_for_console`]):**认领**已有的那一行(邮箱对得上,
    /// 或这台机器还没主人且只有一个本地 owner),而不是在同一台机器上再造一个「第二个我」;
    /// 认不到又确实没主人时,就地成为主人。
    ///
    /// 安全边界:这一档**只**给本地控制台。远端递进来的断言(`/api/collab/login_assertion`)
    /// 走的仍是 [`Admission::ExistingOnly`] —— 陌生账号照旧要邀请码,这条铁律没松。
    LocalConsole,
}

/// 本机控制台首登时,找出「其实就是同一个人」的那一行 users。找不到返回 None。
///
/// 两条证据,强弱分明:
///  ① **邮箱对得上** —— 账号中心保证一个邮箱只属于一个账号,所以这是硬证据,
///     哪怕这台机器已经有主人、有一堆成员,也认。
///  ② 老账号常常压根没绑邮箱(`create_user` 建的本地 owner 就是),于是退而求其次:
///     **这台机器还没有主人(`owner_uid` 空)且全机只有一个 owner、它还是个本地账号** ——
///     这就是「我自己的电脑上那个老账号」。有第二个 owner、或已经有主人,一律不猜。
///
/// `host_owner` 是**进事务之前**读好的 `owner_uid`:meta 表在另一条连接上,
/// 写事务开着的时候再去开一条连接读它是在给自己找 SQLITE_BUSY。
fn adopt_row_for_console(
    tx: &rusqlite::Transaction<'_>,
    claims: &Claims,
    host_owner: &str,
) -> Result<Option<(i64, String)>, String> {
    let email = claims.email.trim().to_ascii_lowercase();
    if !email.is_empty() {
        let hit: Option<(i64, String, String)> = tx
            .query_row(
                "SELECT id,role,uid FROM users WHERE email<>'' AND email=?1 COLLATE NOCASE LIMIT 1",
                params![email],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .map_err(|e| format!("按邮箱找本机账号失败: {e}"))?;
        if let Some((id, role, uid)) = hit {
            // 这一行已经绑在**另一个**云端账号上:不抢。静默顶替等于把别人在本机的
            // 项目关系送给来人 —— 与下面那道同名不同 uid 的闸同一口径。
            if !uid.is_empty() && uid != claims.uid {
                return Err(format!(
                    "本机的「{email}」已经绑在另一个云端账号上了 —— \
                     换回原来那个账号登录,或让 owner 先在成员列表里解绑它"
                ));
            }
            return Ok(Some((id, role)));
        }
    }
    if !host_owner.is_empty() {
        return Ok(None);
    }
    let mut st = tx
        .prepare("SELECT id,uid FROM users WHERE role='owner' ORDER BY id")
        .map_err(|e| format!("查本机 owner 失败: {e}"))?;
    let owners: Vec<(i64, String)> = st
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(|e| format!("查本机 owner 失败: {e}"))?
        .collect::<Result<_, _>>()
        .map_err(|e| format!("查本机 owner 失败: {e}"))?;
    drop(st);
    match owners.as_slice() {
        [(id, uid)] if uid.trim().is_empty() => Ok(Some((*id, "owner".to_string()))),
        _ => Ok(None),
    }
}

/// 「这台机器不认识你」的话术。**必须带「设备码」三字** —— 登录界面靠它把码输入框
/// 自动展开(见 `EmailLogin.vue`)。顺带把「码在哪儿」说死,不让用户自己猜。
///
/// 注意这句话里已经没有「主机」「邀请」「入伙」了:那是我们的内部分工,不是用户的世界。
/// 他的世界只有一件事 —— 「那台机器的码给我」。
fn no_invite_msg() -> String {
    "这台机器还不认得你 —— 在它的「互联」页第 ① 屏复制那串**设备码**(PLR1-…),\
     填到下面就能进"
        .to_string()
}

/// 把一张已验过的断言落成本机的成员资格,返回本机 `users` 行。
///
/// 四种情况:
///  · uid 已在本机 → 更新用户名/邮箱/昵称(权威改名后本机跟着改),复用原角色。
///  · uid 不在本机、本机零账号 → 建号并给 **owner**(装这台机器的人就是它的主人)。
///  · uid 不在本机、带邀请票据 → 按票据角色建号入伙。
///  · uid 不在本机、无票据 → **拒**。云端有账号 ≠ 本机欢迎你。
///
/// 用户名撞车(同名但 uid 不同)会明确报错交给 owner 处理,绝不静默顶替 ——
/// 顶替等于把别人在本机的项目关系送给来人。
pub fn upsert_member(claims: &Claims, admission: Admission) -> Result<User, String> {
    // 进事务之前读:meta 走的是另一条连接,写事务开着时再开一条读它 = 自找 SQLITE_BUSY。
    let host_owner = owner_uid();
    let mut conn = open_db()?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| format!("成员资格事务失败: {e}"))?;

    let existing: Option<(i64, String)> = tx
        .query_row(
            "SELECT id,role FROM users WHERE uid=?1",
            params![claims.uid],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(|e| format!("查成员资格失败: {e}"))?;

    if let Some((id, role)) = existing {
        let disabled: i64 = tx
            .query_row("SELECT disabled FROM users WHERE id=?1", params![id], |r| {
                r.get(0)
            })
            .map_err(|e| format!("查账号状态失败: {e}"))?;
        if disabled != 0 {
            return Err("你的账号在本主机上已被停用".into());
        }
        tx.execute(
            "UPDATE users SET username=?1, email=?2, display_name=?3 WHERE id=?4",
            params![
                claims.username,
                claims.email,
                claims.display_name.trim(),
                id
            ],
        )
        .map_err(|e| {
            if e.to_string().contains("UNIQUE") {
                // 权威那边把邮箱/用户名改成了本机另一个账号已占的值。本机不猜、不顶替。
                "同步账号资料失败:用户名或邮箱与本机另一个账号冲突,请联系 owner 处理".to_string()
            } else {
                format!("同步账号资料失败: {e}")
            }
        })?;
        tx.commit().map_err(|e| format!("提交成员资格失败: {e}"))?;
        // 老库升级路径:这台机器早就有 owner 了,但 owner_uid 这个键是这次才引入的。
        // 由本人登录时补写一次(只写一次,后来者改不动)。
        stamp_owner_uid(&role, &claims.uid);
        return Ok(User {
            id,
            username: claims.username.clone(),
            role,
            display_name: claims.display_name.trim().into(),
            disabled: false,
        });
    }

    // 本机控制台首登:先认领「其实就是同一个人」的那一行,而不是在同一台机器上再建一个号。
    // 认领而不是新建,是为了让老账号名下的项目/任务关系跟着走 —— 那正是用户说的「统一一下」。
    if matches!(admission, Admission::LocalConsole) {
        if let Some((id, role)) = adopt_row_for_console(&tx, claims, &host_owner)? {
            tx.execute(
                "UPDATE users SET uid=?1, username=?2, email=?3, display_name=?4 WHERE id=?5",
                params![
                    claims.uid,
                    claims.username,
                    claims.email,
                    claims.display_name.trim(),
                    id
                ],
            )
            .map_err(|e| {
                if e.to_string().contains("UNIQUE") {
                    format!(
                        "本机另一个账号占着用户名「{}」或这个邮箱 —— 请先把它改名再登录",
                        claims.username
                    )
                } else {
                    format!("认领本机账号失败: {e}")
                }
            })?;
            tx.commit().map_err(|e| format!("提交成员资格失败: {e}"))?;
            stamp_owner_uid(&role, &claims.uid);
            db::audit(&claims.username, "member.adopt", &claims.uid, &role);
            return Ok(User {
                id,
                username: claims.username.clone(),
                role,
                display_name: claims.display_name.trim().into(),
                disabled: false,
            });
        }
    }

    // 同名不同 uid → 拒绝顶替。
    let clash: Option<String> = tx
        .query_row(
            "SELECT uid FROM users WHERE username=?1 COLLATE NOCASE LIMIT 1",
            params![claims.username],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| format!("查用户名失败: {e}"))?;
    if let Some(other_uid) = clash {
        return Err(if other_uid.is_empty() {
            format!(
                "本机已有同名本地账号「{}」——请 owner 先把它改名或迁移,再用云端账号登录",
                claims.username
            )
        } else {
            format!("本机已有另一个云端账号占用用户名「{}」", claims.username)
        });
    }

    let n: i64 = tx
        .query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    // 空库 = 这台主机刚装好还没主人,第一个用云端账号登进来的人就是它的 owner
    // (等价于原来的 bootstrap,只是身份改由云端签)。此后一律要票据。
    let role = if n == 0 {
        "owner".to_string()
    } else {
        match admission {
            Admission::Invited(r) => r,
            Admission::SelfOwned => "owner".to_string(),
            // 认领不到任何旧行,但这台机器确实还没有主人:坐在它跟前的这个人就是主人。
            // 已经有主人了则照旧要邀请码 —— 换个账号在本机控制台登录不该改朝换代。
            Admission::LocalConsole if host_owner.is_empty() => "owner".to_string(),
            Admission::LocalConsole | Admission::ExistingOnly => return Err(no_invite_msg()),
        }
    };
    tx.execute(
        "INSERT INTO users(username,pass_hash,role,display_name,created_at,email,uid) \
         VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![
            claims.username,
            FEDERATED_SENTINEL,
            role,
            claims.display_name.trim(),
            now(),
            claims.email,
            claims.uid
        ],
    )
    .map_err(|e| format!("建成员资格失败: {e}"))?;
    let id = tx.last_insert_rowid();
    tx.commit().map_err(|e| format!("提交成员资格失败: {e}"))?;
    stamp_owner_uid(&role, &claims.uid);
    db::audit(&claims.username, "member.join", &claims.uid, &role);
    Ok(User {
        id,
        username: claims.username.clone(),
        role,
        display_name: claims.display_name.trim().into(),
        disabled: false,
    })
}

/// 这个云端账号在本机**已经有成员资格**了吗。纯本地读,不联网 ——
/// 设备网的对账循环每拍都要问一次「我进得了自己这台机器的门吗」,不能为此打一趟公网。
pub fn is_member(uid: &str) -> bool {
    let uid = uid.trim();
    if uid.is_empty() {
        return false;
    }
    let Ok(conn) = open_db() else { return false };
    conn.query_row(
        "SELECT 1 FROM users WHERE uid=?1 AND disabled=0 LIMIT 1",
        params![uid],
        |_| Ok(()),
    )
    .optional()
    .map(|x| x.is_some())
    .unwrap_or(false)
}

/// 本机记着的主人 uid。空 = 这台机器还没有主人(全新装机)。
pub fn owner_uid() -> String {
    db::meta_get(OWNER_UID_KEY).unwrap_or_default()
}

const OWNER_UID_KEY: &str = "owner_uid";

/// 某个账号成为本机 owner 时,把它的 uid 钉成「这台机器的主人」。
///
/// **只写一次**(见 [`db::meta_set_once`])。这是 [`Admission::SelfOwned`] 整条路的地基:
/// 若这个键能被后来的写入覆盖,那就是「谁最后一个登录谁就是主人」—— 等于把远程夺权做成
/// 一条 API。写不进去(已有主人)不是错误,是正常的日常路径,静默即可。
pub(super) fn stamp_owner_uid(role: &str, uid: &str) {
    if role != "owner" || uid.trim().is_empty() {
        return;
    }
    if let Ok((cur, fresh)) = db::meta_set_once(OWNER_UID_KEY, uid) {
        if fresh {
            db::audit("system", "host.owner_uid.set", uid, "本机主人已确定");
        } else if cur != uid {
            // 同一台机器上有两个 owner 角色的联邦账号:后来的那个不会成为「主人」,
            // 它照样是 owner(能管本机),但同账号设备网的自动准入只认第一个。如实留痕。
            db::audit("system", "host.owner_uid.keep", &cur, uid);
        }
    }
}


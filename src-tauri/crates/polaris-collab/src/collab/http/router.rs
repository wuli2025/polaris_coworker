#[path = "rate_limits.rs"]
mod rate_limits;
use super::*;

// ───────────────────────── 路由表 ─────────────────────────

/// 协作路由。`with_ws=true` 时附带 /ws(桌面 hosting 用;server 壳自己有全量 /ws,传 false 防 merge 撞路由)。
pub fn collab_router(state: CollabState, with_ws: bool) -> Router {
    #[cfg(not(test))]
    crate::collab::interconnect::sync::start();
    let mut r = Router::new()
        // 账号/会话/票据/设备(权限判定全走 collab.db 确定性授权表)
        .route("/api/collab/bootstrap", post(collab_bootstrap))
        .route("/api/collab/login", post(collab_login))
        .route("/api/collab/logout", post(collab_logout))
        .route("/api/collab/me", get(collab_me))
        .route("/api/collab/redeem", post(collab_redeem))
        // ── 账号权威(云机签发身份断言;成员主机离线验签)──
        // 权威侧发号:公钥、注册(强制绑邮箱)、登录、改密。非权威机上这些路由一律 400,
        // 前端据 /api/account/info 决定登录页往哪儿打,不靠猜。
        .route("/api/interconnect/login", post(browser_login))
        .route("/api/interconnect/status", get(browser_status))
        .route("/api/interconnect/retry", post(browser_retry))
        .route("/api/interconnect/security/inbox", get(browser_inbox))
        .route("/api/interconnect/security/ack", post(browser_ack))
        .route("/api/interconnect/security/decide", post(browser_decide))
        .route("/api/interconnect/devices/revoke", post(browser_revoke))
        .route("/api/account/info", get(account_info))
        .route("/api/account/pubkey", get(account_pubkey))
        .route("/api/account/signup", post(account_signup))
        .route("/api/account/login", post(account_login))
        // 全局验证码登录:邮箱 + 验证码 → 身份断言(免密码,凭邮箱所有权)。
        // send_code 是整套账号体系**唯一**的公开发码口;login_code 首登即自动建号。
        .route("/api/account/send_code", post(account_send_code))
        .route("/api/account/login_code", post(account_login_code))
        .route("/api/account/reset", post(account_reset))
        // 成员侧进门:拿权威签的断言换本机会话。
        .route("/api/collab/login_assertion", post(collab_login_assertion))
        // ── 同账号设备网(权威侧目录:登录即成网,不必再粘连接码)──
        .route("/api/mesh/enroll", post(mesh_enroll))
        .route("/api/mesh/announce", post(mesh_announce))
        .route("/api/mesh/security/inbox", get(security_inbox))
        .route("/api/mesh/security/watch", get(security_watch))
        .route("/api/mesh/security/ack", post(security_ack))
        .route("/api/mesh/risk/authorize", post(security_authorize))
        .route("/api/mesh/risk/decide", post(security_decide))
        .route("/api/mesh/revocations", get(mesh_revocations))
        .route("/api/mesh/assert", post(mesh_assert))
        .route("/api/mesh/revoke", post(mesh_revoke))
        .route("/api/mesh/devices", get(mesh_devices))
        .route("/api/mesh/rename", post(mesh_rename))
        // ── 设备信任契约(**本机**的权限面,云端碰不到)──
        .route("/api/peer/grants", get(peer_grants_list))
        .route("/api/peer/grant", post(peer_grant_set))
        .route("/api/peer/trust", post(peer_grant_trust))
        .route("/api/peer/revoke", post(peer_grant_revoke))
        .route("/api/collab/join", post(collab_join))
        .route("/api/collab/authority/repin", post(collab_authority_repin))
        // GitHub 式:开放注册 + 用户名搜索 + 团队(一人多团队,团队下挂项目)
        .route("/api/collab/signup", post(collab_signup))
        // 邮箱验证码:注册 / 找回密码(验证码 = 邮箱所有权证明;频控在 mail 模块)
        .route("/api/collab/email/status", get(collab_email_status))
        .route("/api/collab/email/send_code", post(collab_email_send_code))
        .route("/api/collab/email/signup", post(collab_email_signup))
        .route("/api/collab/email/reset", post(collab_email_reset))
        // 本机验证码登录(账号权威/本地账号模式;成员主机不开)
        .route("/api/collab/email/login", post(collab_email_login))
        .route(
            "/api/collab/admin/email_config",
            get(collab_admin_email_config_get).post(collab_admin_email_config_set),
        )
        .route("/api/collab/users/search", get(collab_user_search))
        .route("/api/collab/teams", get(team_list).post(team_create))
        .route(
            "/api/collab/team/members",
            get(team_members_api).post(team_member_add),
        )
        .route("/api/collab/team/member_remove", post(team_member_remove))
        .route("/api/collab/admin/ticket", post(collab_ticket))
        .route("/api/collab/admin/users", get(collab_users))
        .route("/api/collab/admin/user_disable", post(collab_user_disable))
        .route(
            "/api/collab/admin/user_reset_password",
            post(collab_admin_user_reset_password),
        )
        // owner 远程账号管理(建/改/删/补签 uid)
        .route(
            "/api/collab/admin/account_create",
            post(collab_admin_account_create),
        )
        .route(
            "/api/collab/admin/account_update",
            post(collab_admin_account_update),
        )
        .route(
            "/api/collab/admin/account_delete",
            post(collab_admin_account_delete),
        )
        .route(
            "/api/collab/admin/account_uid_backfill",
            post(collab_admin_account_uid_backfill),
        )
        .route("/api/collab/admin/devices", get(collab_devices))
        .route("/api/collab/admin/audit", get(collab_audit_recent))
        // 遥测帧本就该 <2KB:单独收紧到 4KB body 上限,低权限成员无法用大包耗解析(codex #3)。
        .route(
            "/api/collab/telemetry",
            post(collab_telemetry_report).layer(axum::extract::DefaultBodyLimit::max(4096)),
        )
        .route(
            "/api/collab/admin/device_revoke",
            post(collab_device_revoke),
        )
        // 账号镜像:本地权威+云端密文兜底(零知识,解密钥永不出主机)
        .route("/api/collab/mirror/export", post(mirror_export_api))
        .route("/api/collab/mirror/store", post(mirror_store_api))
        .route("/api/collab/mirror/pull", get(mirror_pull_api))
        .route("/api/collab/mirror/restore", post(mirror_restore_api))
        // 项目与任务卡(六态状态机;权限先过项目成员表)
        .route(
            "/api/collab/projects",
            get(project_list).post(project_create),
        )
        .route(
            "/api/collab/projects/shared-scope",
            post(project_shared_scope_set),
        )
        .route(
            "/api/collab/project/members",
            get(project_members).post(project_member_add),
        )
        .route("/api/collab/project/lead", post(project_set_lead))
        .route("/api/collab/tasks", get(task_list).post(task_create))
        // 任务级对话:增量拉取 + 发消息 + 主 Agent 手动回复
        .route(
            "/api/collab/tasks/:id/messages",
            get(task_messages_get).post(task_messages_post),
        )
        .route("/api/collab/tasks/:id/ai-reply", post(task_ai_reply))
        .route("/api/collab/task/claim", post(task_claim))
        .route("/api/collab/task/submit", post(task_submit))
        .route("/api/collab/task/review", post(task_review))
        .route("/api/collab/task/rounds", get(task_rounds))
        .route("/api/collab/task/archive", post(task_archive))
        .route("/api/collab/task/cancel", post(task_cancel))
        .route("/api/collab/activity", get(collab_activity))
        // 检查工作流(CI-lite):按轮读结果 / 手动重跑 / 项目档位
        .route("/api/collab/checks", get(checks_get))
        .route("/api/collab/checks/rerun", post(checks_rerun))
        .route("/api/collab/checks/profile", post(checks_profile_set))
        .route("/api/collab/checks/skills", get(checks_skills_list))
        // 主 Agent:授权位 / 晨会 / 改派 / 催办 / 模型配置 / AI 拆卡·验收·融合
        .route(
            "/api/collab/lead/grants",
            get(lead_grants_get).post(lead_grants_set),
        )
        .route("/api/collab/lead/morning", get(lead_morning))
        .route("/api/collab/lead/assign", post(lead_assign_api))
        .route("/api/collab/lead/nudge", post(lead_nudge_api))
        .route(
            "/api/collab/lead/model",
            get(lead_model_get).post(lead_model_set),
        )
        .route("/api/collab/lead/ai/decompose", post(lead_ai_decompose))
        .route("/api/collab/lead/ai/review", post(lead_ai_review))
        .route("/api/collab/lead/ai/fuse", post(lead_ai_fuse))
        // 合并闸门(冲突试算/裁决/squash 放行/以卡回滚)
        .route("/api/collab/merge/trial", post(merge_trial_api))
        .route("/api/collab/merge/resolve", post(merge_resolve_api))
        .route("/api/collab/merge/squash", post(merge_squash_api))
        .route("/api/collab/merge/revert", post(merge_revert_api))
        // Gitea 托管(owner)与 git smart HTTP 反代(隧道只放行 8080 一个端口的关键)
        .route("/api/collab/gitea/status", get(gitea_status_api))
        .route("/api/collab/gitea/start", post(gitea_start_api))
        .route("/api/collab/gitea/stop", post(gitea_stop_api))
        .route(
            "/git/*rest",
            axum::routing::any(git_proxy)
                .layer(axum::extract::DefaultBodyLimit::max(512 * 1024 * 1024)),
        )
        // iroh 隧道(collab-net 编译才有):主机侧监听 + 状态 + RelayMap 动态下发
        .route("/api/collab/tunnel/status", get(tunnel_status_api))
        .route("/api/collab/tunnel/start", post(tunnel_start_api))
        .route("/api/collab/tunnel/relays", post(tunnel_relays_api))
        // 设备码(一码):公开的敲门口 + owner 才能读/换的本机码。
        .route(
            "/api/collab/connect/challenge",
            post(collab_connect_challenge),
        )
        .route("/api/collab/connect", post(collab_connect))
        .route("/api/collab/code", get(collab_code_get))
        .route("/api/collab/code/rotate", post(collab_code_rotate))
        // 远程文件浏览(fsface):角色或设备契约 + 路径关押,随隧道透传到对端的盘。
        .route("/api/fs/list", get(fs_list_api))
        .route("/api/fs/read", get(fs_read_api))
        .route("/api/fs/stat", get(fs_stat_api))
        .route("/api/fs/caps", get(fs_caps_api))
        // 写面:逐根写位 + 角色闸双闸。挂载盘的 PUT/MKCOL/DELETE/MOVE/COPY 落到这里。
        // 4GB 是 Windows WebDAV 重定向器的协议顶格,body limit 对齐它。
        .route(
            "/api/fs/write",
            axum::routing::put(fs_write_api)
                .layer(axum::extract::DefaultBodyLimit::max(4 * 1024 * 1024 * 1024)),
        )
        .route("/api/fs/op/:op", post(fs_op_api));
    // 云机中继网关(collab-net 才有):主机挂牌 + /h/:id 反代到该主机(经 iroh 到桌面 apihub)。
    #[cfg(feature = "collab-net")]
    {
        r = r.route("/api/gw/register", post(gw_register)).route(
            "/h/:host_id/*rest",
            axum::routing::any(crate::collab::gateway::gateway_proxy)
                .layer(axum::extract::DefaultBodyLimit::max(512 * 1024 * 1024)),
        );
    }
    if with_ws {
        r = r.route("/ws", get(collab_ws_handler));
    }
    r.layer(axum::middleware::from_fn(risk_middleware))
        .layer(axum::middleware::from_fn(rate_limits::middleware))
        .layer(axum::middleware::from_fn(browser_no_store)).with_state(state)
}

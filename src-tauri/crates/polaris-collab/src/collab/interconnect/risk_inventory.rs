//! Policy B entry-point inventory consumed by HTTP and invoke enforcement adapters.
//! Local console wrappers use the same operation IDs; gate.rs owns authorization.
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskOperation {
    pub id: &'static str,
    pub entry_points: &'static [&'static str],
    pub reason: &'static str,
}

pub const HIGH_RISK_OPERATIONS: &[RiskOperation] = &[
    RiskOperation {
        id: "remove_remote_storage",
        entry_points: &[
            "fs_unmount", "fs_share_set", "fs_share_save", "POST /api/fs/op/delete",
            "mesh_device_remove", "mesh_leave",
        ],
        reason: "移除远程挂载或删除远程文件，可能造成数据丢失或撤去其他设备访问入口",
    },
    RiskOperation {
        id: "change_device_permissions",
        entry_points: &[
            "peer_grant_set", "peer_trust", "fs_share_save", "POST /api/peer/grant", "POST /api/peer/trust",
            "POST /api/collab/admin/account_update",
        ],
        reason: "变更盘读写、执行许可、自动挂载或用户角色，扩大访问权限",
    },
    RiskOperation {
        id: "revoke_other_device",
        entry_points: &[
            "mesh_kick", "mesh_device_remove", "peer_revoke", "POST /api/mesh/revoke",
            "POST /api/peer/revoke", "POST /api/collab/admin/device_revoke",
        ],
        reason: "新设备可以撤销旧设备，使旧设备无法再批准或否决请求",
    },
    RiskOperation {
        id: "change_password_or_email",
        entry_points: &[
            "POST /api/account/reset", "POST /api/collab/email/reset",
            "POST /api/collab/admin/user_reset_password", "POST /api/collab/admin/account_update",
        ],
        reason: "更换账号认证或找回凭据，可使原持有人失去账号控制权",
    },
    RiskOperation {
        id: "delete_account",
        entry_points: &["POST /api/collab/admin/account_delete"],
        reason: "删除账号并级联撤销其会话、设备和成员关系",
    },
];

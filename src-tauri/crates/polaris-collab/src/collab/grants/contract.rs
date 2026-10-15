//! Contract view and effective permissions (policy B has no automatic cooldown).
use super::*;

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
    /// Legacy storage/serialization only; policy B ignores this timestamp.
    pub cooldown_until: i64,
    pub granted_at: i64,
    pub granted_by: String,
    pub revoked: bool,
}

impl Grant {
    /// Effective disk permission respects explicit restrictions and revocation.
    pub fn effective_fs(&self) -> &str {
        if self.revoked {
            return FS_NONE;
        }
        &self.fs_access
    }

    /// Effective execution permission respects explicit restrictions and revocation.
    pub fn effective_exec(&self) -> &str {
        if self.revoked {
            return EXEC_NONE;
        }
        &self.exec_access
    }

    /// 这台设备现在该不该被自动连上并挂盘。
    pub fn should_mount(&self) -> bool {
        !self.revoked && self.auto_mount && self.effective_fs() != FS_NONE
    }
}


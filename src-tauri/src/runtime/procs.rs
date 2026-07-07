//! 子进程池与进程树管理的单一实现。
//!
//! 此前 chat.rs 与 doctor.rs 各持一份 `CHILDREN` 静态池、chat.rs 与 project.rs
//! 各持一份 `kill_tree`、三处 `no_window` —— 全部收口到这里。
//! req_id 命名空间由调用方保证不冲突(chat 用 `req-*`,doctor 用 `env-*`)。

use once_cell::sync::Lazy;
use parking_lot::{Mutex, MutexGuard};
use std::collections::{HashMap, HashSet};
use std::process::{Child, Command};

/// 全局子进程注册表:所有「按 req_id 追踪、退出时必须回收」的子进程都登记在此。
/// App 退出钩子调用 [`kill_all`] 统一收割,不再依赖各模块自扫门前雪。
pub static CHILDREN: Lazy<ChildRegistry> = Lazy::new(ChildRegistry::new);

/// 「取消挂起」标记:stop 请求可能在 child 注册进池**之前**到达 —— 此时按 id 找不到
/// child,就把 req_id 记到这里;spawn 管线在注册前后各查一次,保证窄窗口内不漏杀。
pub struct ChildRegistry {
    map: Mutex<HashMap<String, Child>>,
    pending_cancel: Mutex<HashSet<String>>,
}

impl ChildRegistry {
    fn new() -> Self {
        Self {
            map: Mutex::new(HashMap::new()),
            pending_cancel: Mutex::new(HashSet::new()),
        }
    }

    /// 直接拿池锁(watchdog 等需要遍历/组合操作的调用方使用)。
    pub fn lock(&self) -> MutexGuard<'_, HashMap<String, Child>> {
        self.map.lock()
    }

    pub fn insert(&self, req_id: impl Into<String>, child: Child) {
        self.map.lock().insert(req_id.into(), child);
    }

    pub fn remove(&self, req_id: &str) -> Option<Child> {
        self.map.lock().remove(req_id)
    }

    /// 摘出并杀掉(含整棵进程树)。返回是否找到了该 child。
    pub fn kill(&self, req_id: &str) -> bool {
        match self.map.lock().remove(req_id) {
            Some(mut child) => {
                kill_tree(child.id());
                let _ = child.kill();
                let _ = child.wait();
                true
            }
            None => false,
        }
    }

    /// App 退出时回收所有在飞子进程,连同它们扇出的整棵进程树。
    /// 否则用户关 App 时,长任务拉起的 dev server / node / python 会变孤儿。
    pub fn kill_all(&self) {
        let mut map = self.map.lock();
        for (_id, mut child) in map.drain() {
            kill_tree(child.id());
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// 标记「取消挂起」(child 尚未注册时的 stop)。
    pub fn mark_cancel(&self, req_id: impl Into<String>) {
        self.pending_cancel.lock().insert(req_id.into());
    }

    /// 消费「取消挂起」标记:有则移除并返回 true。
    pub fn take_cancel(&self, req_id: &str) -> bool {
        self.pending_cancel.lock().remove(req_id)
    }
}

/// 按 PID kill 整个进程树。子进程在 shell/Task 工具下会拉起 python/node/dev server
/// 等子孙,只 kill 本体会留孤儿占着端口。
pub fn kill_tree(pid: u32) {
    #[cfg(windows)]
    {
        let mut cmd = Command::new("taskkill");
        cmd.args(["/PID", &pid.to_string(), "/T", "/F"]);
        no_window(&mut cmd);
        let _ = cmd.output();
    }
    #[cfg(not(windows))]
    {
        // 杀进程组 (shell -c 起的子孙); 失败再退化为 kill 单进程。
        let _ = Command::new("kill")
            .args(["-TERM", &format!("-{}", pid)])
            .output()
            .or_else(|_| Command::new("kill").arg(pid.to_string()).output());
    }
}

/// Windows 下抑制子进程闪黑框(CREATE_NO_WINDOW);其它平台是 no-op。
#[cfg(windows)]
pub fn no_window(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x0800_0000);
}
#[cfg(not(windows))]
pub fn no_window(_cmd: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn sleeper() -> Command {
        #[cfg(windows)]
        {
            let mut c = Command::new("cmd");
            c.args(["/C", "ping -n 30 127.0.0.1 > NUL"]);
            c
        }
        #[cfg(not(windows))]
        {
            let mut c = Command::new("sleep");
            c.arg("30");
            c
        }
    }

    #[test]
    fn registry_insert_kill_roundtrip() {
        let reg = ChildRegistry::new();
        let mut cmd = sleeper();
        no_window(&mut cmd);
        let child = cmd.spawn().expect("spawn sleeper");
        reg.insert("t-1", child);
        assert!(reg.kill("t-1"), "应能找到并杀掉已注册 child");
        assert!(!reg.kill("t-1"), "重复 kill 应返回 false");
        assert!(reg.remove("t-1").is_none());
    }

    #[test]
    fn pending_cancel_is_consumed_once() {
        let reg = ChildRegistry::new();
        assert!(!reg.take_cancel("x"));
        reg.mark_cancel("x");
        assert!(reg.take_cancel("x"), "标记后第一次消费应为 true");
        assert!(!reg.take_cancel("x"), "消费后标记应清除");
    }

    #[test]
    fn kill_all_drains_pool() {
        let reg = ChildRegistry::new();
        for i in 0..2 {
            let mut cmd = sleeper();
            no_window(&mut cmd);
            reg.insert(format!("t-{i}"), cmd.spawn().expect("spawn"));
        }
        reg.kill_all();
        assert!(reg.lock().is_empty());
    }
}

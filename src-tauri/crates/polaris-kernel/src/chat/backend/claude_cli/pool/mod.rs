//! 常驻 claude agent 进程池 (conv_id → 常驻子进程)。
//!
//! 为什么要常驻: claude CLI 是 bun 打包的单文件, 每次 spawn 自举 ~6.4s(解包+加载,
//! 外部无法加速), 旧路径每条消息都重新 spawn → 首 token 8-10s。改用
//! `--input-format stream-json` 让进程常驻: 每轮往 stdin 写一行 user 消息,
//! 实测第 2 轮起首事件 1.4-2.6s(还吃到服务端 prompt cache)。每轮以 CLI 输出的
//! `{"type":"result",...}` 事件为收尾界标。
//!
//! 语义要点(与旧「每轮重发全量 prompt」的差异):
//! - **首条消息**(或进程重建后的第一条)发完整拼装 prompt(系统前置+KB+历史);
//! - **续轮**只发「本轮新增部分」: claude 进程内自己保持上下文, 会话级不变的
//!   系统前置/CLAUDE.md/历史 不再重发(见 [`PromptPlan`] 的三类分段)。
//! - 配置指纹([`Fingerprint`])变了 → 旧进程语义失效(权限/供应商/模型/工具面都
//!   烙在进程启动参数里), 只能杀掉重建, 本条按首条处理。
//!
//! 生命周期: LRU 上限 2 个常驻进程(每个 ~200-400MB 内存); 空闲 10 分钟 TTL 惰性
//! 收割(每次发消息时顺手扫, 不开常驻轮询线程 —— 多留几分钟只是内存, 不烧 CPU);
//! 进程死亡/stdin 写失败 → 透明重建不报错; 停止按钮/对话删除/应用退出 → 杀进程,
//! 下轮自动重建。开关: POLARIS_PERSISTENT_AGENT(默认开, =0 回落旧的每消息 spawn)。

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStderr, ChildStdout};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Instant, SystemTime};

#[cfg(not(feature = "desktop"))]
use crate::host::AppHandle;
#[cfg(feature = "desktop")]
use tauri::AppHandle;

use crate::runtime::procs::{kill_tree, CHILDREN};

use super::pipeline;
use crate::chat::types::ChatStreamEvent;

struct SessionShared {
    conv: String,
    pid: u32,
    /// 进程仍可用(EOF/主动杀后置 false; 池条目下轮惰性清理)。
    alive: AtomicBool,
    /// 本次死亡已由取消/看门狗主动执行并自行告知前端 → reader 收 EOF 不再报错
    /// (对齐旧路径「chat_cancel 先摘 Child, reader 查不到就不发异常退出 error」的语义)。
    kill_reported: AtomicBool,
    /// 当前活跃轮的 req_id: 看门狗以「它被清空」为本轮结束界标 —— 轮与轮之间的
    /// 静默不是卡死, 绝不能被 idle 杀。
    active_req: Mutex<Option<String>>,
    /// 当前活跃轮的看门狗(stderr 线程按行 touch 用; 轮结束置 None)。
    watchdog: Mutex<Option<Arc<polaris_watchdog::Watchdog>>>,
    /// stderr 尾巴(封顶), 进程猝死时拼进错误信息辅助诊断。
    stderr_tail: Mutex<String>,
}

/// 一个常驻会话进程: 池锁下只做轻操作, kill/wait/写 stdin 全在池锁外。
struct SessionProc {
    /// 摘出后在锁外 kill/wait(所有权在手 = reader 无法先 reap → pid 不会被复用误杀)。
    child: Option<Child>,
    stdin: Arc<Mutex<ChildStdin>>,
    job_tx: mpsc::Sender<TurnJob>,
    shared: Arc<SessionShared>,
    /// 已发过的门控块 key(见 PromptPlan::Keyed)。
    sent_keys: HashSet<String>,
    /// 首条消息(完整 prompt)是否已成功写入 —— 续轮判定依据。
    first_done: bool,
}

static POOL: Lazy<Mutex<PoolCore<SessionProc>>> = Lazy::new(|| Mutex::new(PoolCore::new()));

/// 单调毫秒时钟(进程启动起算): 只做 LRU/TTL 相对比较, 不需要墙钟。
fn now_ms() -> u64 {
    static START: Lazy<Instant> = Lazy::new(Instant::now);
    START.elapsed().as_millis() as u64
}

/// 收割一个会话进程(池锁外调用): 标记死亡→杀整树→reap。
/// reported=true 表示"死因已由调用方告知前端"(取消/看门狗/退出), reader 不再补报 error。
/// 「正在跑轮次」判定(TTL 清扫/LRU 逐出的豁免谓词): 进程活着且本轮 active_req 未被
/// reader 清空。进程死了(alive=false)即便 active_req 残留也不算忙 —— 死条目必须可清。
fn session_busy(sp: &SessionProc) -> bool {
    sp.shared.alive.load(Ordering::SeqCst) && sp.shared.active_req.lock().is_some()
}

fn reap(mut s: SessionProc, reported: bool) {
    if reported {
        s.shared.kill_reported.store(true, Ordering::SeqCst);
    }
    s.shared.alive.store(false, Ordering::SeqCst);
    if let Some(mut c) = s.child.take() {
        kill_tree(s.shared.pid);
        let _ = c.kill();
        let _ = c.wait();
    }
}

// ───────────────────────── 对外收割入口 ─────────────────────────

/// 停止按钮: req_id 命中某会话的活跃轮 → 杀常驻进程(简单可靠, 下轮自动重建)。
/// 返回 false = 不是常驻轮(旧路径/或尚未注册), 调用方走旧的 CHILDREN 逻辑。
pub fn cancel_by_req(req_id: &str) -> bool {
    let victim = {
        let mut g = POOL.lock();
        let conv = g
            .iter_mut()
            .find(|(_, sp)| sp.shared.active_req.lock().as_deref() == Some(req_id))
            .map(|(c, _)| c.to_string());
        conv.and_then(|c| g.remove(&c))
    };
    match victim {
        Some(s) => {
            // 先标记再杀: reader 收 EOF 时按「已告知」处理, 只发 done 不发 error
            // (与旧路径 chat_cancel 的静默收尾语义一致)。
            reap(s, true);
            true
        }
        None => false,
    }
}

/// 某 req 是否仍是池内某会话的活跃轮(前端「无声死亡」看门狗查询用)。
pub fn is_active_req(req_id: &str) -> bool {
    let mut g = POOL.lock();
    let active = g.iter_mut().any(|(_, sp)| {
        sp.shared.alive.load(Ordering::SeqCst)
            && sp.shared.active_req.lock().as_deref() == Some(req_id)
    });
    active
}

/// 对话删除时收割其常驻进程(没有则无事)。
pub fn kill_conv(conv_id: &str) {
    let victim = POOL.lock().remove(conv_id);
    if let Some(s) = victim {
        reap(s, true);
    }
}

/// 应用退出时收割全部常驻进程(接 App 退出钩子, 与 CHILDREN.kill_all 并列)。
pub fn kill_all() {
    let all = POOL.lock().drain_all();
    for s in all {
        reap(s, true);
    }
}

// ───────────────────────── 本轮任务 ─────────────────────────

/// 发给会话 reader 线程的「一轮」上下文: reader 常驻在 stdout 上, 每收到一个 job
/// 就以该轮的 req_id/产物快照/看门狗处理事件流, 直到 result 事件收尾。
struct TurnJob {
    completion: super::completion::TurnGuard,
    req_id: String,
    conv_id: String,
    app: AppHandle,
    art_dir: PathBuf,
    art_before: HashMap<PathBuf, SystemTime>,
    image_notice: Option<String>,
    watchdog: Arc<polaris_watchdog::Watchdog>,
}

/// run_persistent_turn 的入参(从 chat_send_pipeline 拼装现场带过来)。
pub(crate) struct TurnParams {
    pub app: AppHandle,
    pub req_id: String,
    pub conv_id: String,
    pub plan: PromptPlan,
    pub perm: String,
    pub art_dir: PathBuf,
    pub art_before: HashMap<PathBuf, SystemTime>,
    pub image_notice: Option<String>,
    pub with_task: bool,
    pub work_full: bool,
    pub provider_id: Option<String>,
    pub creative: bool,
    /// 本项目绑定的工作目录(用户选的真实仓库), None=用默认 cwd。透传给 spawn_on_host + 进指纹。
    pub work_dir: Option<String>,
}

/// 由本轮参数算配置指纹 —— run_persistent_turn 与 prewarm 必须用**同一个**算法,
/// 否则预热起的进程会因指纹算法差异被真发送误判「不符」而白白杀掉重建。
fn fingerprint_of(p: &TurnParams) -> Fingerprint {
    // cwd 与 add_dirs 必须用**同一个** effective_cwd 口径算(add_dirs 靠它判 KB/产物在不在
    // cwd 子树), 且这里算出的 cwd 必须与 spawn_session→spawn_on_host 真用的 cwd 完全一致。
    let cwd = pipeline::effective_cwd(p.work_dir.as_deref().map(std::path::Path::new));
    Fingerprint {
        perm: p.perm.clone(),
        // None/"auto"/"" 都是「跟随全局当前供应商」同一档, 归一化后指纹才稳定。
        provider_id: match p.provider_id.as_deref() {
            None | Some("") | Some("auto") => "auto".into(),
            Some(x) => x.into(),
        },
        work_full: p.work_full,
        with_task: p.with_task,
        model: pipeline::model_for_mode(p.work_full).unwrap_or_default(),
        add_dirs: pipeline::extra_claude_dirs(&cwd, &p.art_dir),
        creative: p.creative,
        cwd: cwd.to_string_lossy().to_string(),
    }
}

/// 预热入口(chat_prewarm 命令 → 后台线程调用): 用户还在打字时就把常驻进程起好。
///
/// 为什么有效: claude CLI spawn 后**立即自举**(~6.4s bun 解包+加载), 不等首条 stdin
/// 消息(实测) —— 把这段自举挪进「用户打字期间」, 对话首条消息的首响即可从 ~10s 压到
/// ~3s。语义:
/// - 池里已有该 conv 的活进程且指纹一致 → no-op(不 touch LRU: 预热不算"使用");
/// - 否则按当前设置 spawn 一个空会话入池(first_done=false, 不发任何消息), 之后
///   run_persistent_turn 检出指纹一致直接复用、照常发全量首条 prompt;
/// - creative 取决于本轮 prompt 命中哪些 skill, 预热时还没有 prompt, 按 false 预估
///   (绝大多数轮次非创作); 猜错时真发送检出指纹不符照常杀旧重建 —— 预热白做但无害。
///
/// best-effort: 任何失败只打日志不上抛(预热失败 ≠ 对话失败, 真发送有完整重建路径)。

mod plan;
mod core;
mod lifecycle;
mod reader;
#[cfg(test)]
mod tests;
pub(crate) use plan::*;
use core::*;
pub use lifecycle::*;
use reader::*;

//! Public chat commands; backend adapters own process and protocol handling.

use crate::conv;
#[cfg(not(feature = "desktop"))]
use crate::host::AppHandle;
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
#[cfg(feature = "desktop")]
use tauri::AppHandle;

use crate::runtime::procs::CHILDREN;
use super::backend::bridge_emit as emit_event;

use super::artifacts::*;
use super::backend::claude_cli::pool as session_pool;
use super::types::*;
static REQ_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Request IDs remain compatible with existing frontend routing.
fn next_req_id() -> String {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let c = REQ_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("req-{:x}-{:x}", ts, c)
}

// ───────────────────────── Commands ──────────────────────

#[cfg_attr(feature = "desktop", tauri::command)]
pub async fn chat_send(app: AppHandle, args: ChatSendArgs) -> Result<String, String> {
    chat_send_owned(app, args, "desktop".into()).await
}

pub async fn chat_send_owned(app: AppHandle, args: ChatSendArgs, owner: String) -> Result<String, String> {
    if args.prompt.trim().is_empty() {
        return Err("消息不能为空".into());
    }
    // HTTP body 上限是上传场景共用的 512MB，不能拿它当聊天输入上限；否则单请求即可让
    // prompt 拼装/历史落盘/CLI 参数复制产生数倍内存峰值。2MiB 已远高于正常对话需要。
    if args.prompt.len() > 2 * 1024 * 1024 {
        return Err("消息过大，单次最多 2MB；请改为上传文件后让 AI 读取".into());
    }
    if let Some(cid) = args.conversation_id.as_deref() {
        // 远程客户端(手机/中继)用本地生成的 convId,服务端 conv 表可能没有 → 自动建;
        // 桌面 UI 先 conv_create 再发,走「已存在」分支,行为不变。
        conv::ensure_writable_or_create(cid)?;
    }
    let req_id = next_req_id();
    let lease = super::live_input::reserve(&req_id, args.conversation_id.as_deref(), &owner)?;

    // 轻量同步部分到此为止: 只做 req_id 生成 + user 消息落历史(便宜, 且保证「先 user
    // 后 assistant」的落库顺序)。其余重活 —— 产物目录快照(WalkDir)、CLAUDE.md 渲染、
    // 静态指令拼装、KB 强制召回(含网络嵌入往返 250ms~1.8s)、meta emit、spawn claude、
    // reader 线程挂接 —— 全部挪进后台线程, chat_send 立即返回 req_id,
    // 用户「点发送 → 气泡出现响应」之间不再被这些活钉死。
    if let Some(cid) = &args.conversation_id {
        conv::append_message(cid, "user", &args.prompt).map_err(|e| e.to_string())?;
    }

    let bg_app = app.clone();
    let bg_req = req_id.clone();
    let bg_conv = args.conversation_id.clone();
    std::thread::spawn(move || {
        let _lease = lease;
        // 后台线程内任何 Err/panic 都必须转成前端已能处理的事件发出去, 绝不能静默吞掉
        // 让气泡永远转圈: 旧同步路径里 spawn 失败 = invoke reject = 前端「[发送失败]」气泡
        // + 结束运行态; 这里等价复刻为 error 事件(错误气泡) + done 事件(唯一终态, 结束
        // 运行态/清 reqId)。前端 15s 无声死亡看门狗只是兜底, 不依赖它。
        let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            super::backend::run_chat(&bg_app, &bg_req, args)
        }));
        let err_msg: Option<String> = match run {
            Ok(Ok(())) => None,
            Ok(Err(e)) => Some(e),
            Err(p) => {
                let m = p
                    .downcast_ref::<&str>()
                    .map(|s| s.to_string())
                    .or_else(|| p.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "unknown panic".into());
                Some(format!("对话后台准备阶段异常 (panic): {m}"))
            }
        };
        if let Some(msg) = err_msg {
            emit_event(
                &bg_app,
                ChatStreamEvent {
                    call_id: None, parent_call_id: None,
                    req_id: bg_req.clone(),
                    kind: "error".into(),
                    text: Some(msg),
                    tool: None,
                    conversation_id: bg_conv.clone(),
                },
            );
            emit_event(
                &bg_app,
                ChatStreamEvent {
                    call_id: None, parent_call_id: None,
                    req_id: bg_req.clone(),
                    kind: "done".into(),
                    text: None,
                    tool: None,
                    conversation_id: bg_conv,
                },
            );
            // 失败收尾时顺手清掉可能残留的「取消挂起」标记, 防 PENDING_CANCEL 积攒。
            CHILDREN.take_cancel(&bg_req);
        }
    });

    Ok(req_id)
}

/// 预热常驻 claude 进程(best-effort, **永不报错**): 前端在「打开/切换对话」「输入框
/// 首次聚焦/键入」时调用 —— claude CLI spawn 后立即自举(~6.4s), 不等首条消息, 把
/// 自举挪进用户打字期间, 首条消息首响可从 ~10s 压到 ~3s。
/// 永远 Ok: 预热失败对话照常能发(run_persistent_turn 有完整的重建路径), 报错只会
/// 让前端多一条无意义的错误分支; spawn 本身丢后台线程, 命令即刻返回不占 IPC。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn chat_prewarm(app: AppHandle, args: ChatPrewarmArgs) -> Result<(), String> {
    if super::backend::selected().map_err(|e| e.to_string())?.status().backend != "claude-cli" { return Ok(()); }
    let conv_id = args.conversation_id.trim().to_string();
    if conv_id.is_empty() || !session_pool::persistent_enabled() {
        return Ok(());
    }
    let work_full = args.work_mode.as_deref() == Some("work");
    // 权限档位没传(触发点拿不到输入区状态)时, 套前端 applyModeDefaults 同款默认:
    // work=手动(default) / fast=自动批准(acceptEdits) —— 指纹与随后真发送对齐。
    let perm = match &args.permission_mode {
        Some(pm) => pm.cli_value().to_string(),
        None => if work_full { "default" } else { "acceptEdits" }.to_string(),
    };
    std::thread::spawn(move || {
        session_pool::prewarm(
            app,
            conv_id,
            perm,
            args.dynamic_workflow,
            work_full,
            args.provider_id,
        );
    });
    Ok(())
}

/// Cancel the selected backend, including requests still preparing their prompt.
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn chat_cancel(req_id: String) -> Result<(), String> {
    CHILDREN.mark_cancel(req_id.clone());
    super::backend::selected().and_then(|b| b.cancel(&req_id)).map_err(|e| e.to_string())
}

/// 某轮是否仍在后端真实运行(常驻池活跃轮 或 旧路径子进程仍在册)。
/// 前端「无声死亡」看门狗静默超阈后**先问这里**:仍在跑 → 继续等(长工具零输出静默是
/// 常态,判死交给后端看门狗 —— 它有进程树深检);确认不在了才熔断收尾。
/// 没有这一问,前端定长超时单方面放弃会造成同会话重叠轮次(旧轮还在产出,分批编排
/// 已把下一批发进同一常驻进程,active_req 被顶掉、输出错误归属)。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn chat_is_running(req_id: String) -> bool {
    super::backend::selected().map(|b| b.is_running(&req_id)).unwrap_or(false)
}

/// 读取某会话的分批构建清单 `polaris.build.json`(分批长任务的断点/进度凭据)。
/// 前端编排循环每轮结束后读它, 算还剩几个 pending 来决定续不续、断了从哪接。
/// 不存在或解析失败返回 None(前端据此判定「还没规划」或「读不到, 当作未完成重试」)。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn chat_build_manifest(conversation_id: Option<String>) -> Option<Value> {
    let path = artifacts_dir(conversation_id.as_deref()).join("polaris.build.json");
    let txt = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str::<Value>(&txt).ok()
}

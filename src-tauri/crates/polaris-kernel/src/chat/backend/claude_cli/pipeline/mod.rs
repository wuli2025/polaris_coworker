//! chat_send 主流程: 后台管线(拼 prompt -> spawn claude -> stream reader)、
//! 看门狗、取消、流事件解析。(从 chat.rs 纯移动拆出, 逻辑零变化)

use crate::claude_md;
use crate::conv;
#[cfg(not(feature = "desktop"))]
use crate::host::AppHandle;
use crate::skills;
use parking_lot::Mutex;
use serde_json::Value;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::SystemTime;
#[cfg(feature = "desktop")]
use tauri::{AppHandle, Emitter};

use crate::runtime::procs::no_window;

use crate::chat::artifacts::*;
use crate::chat::prompt::*;
use super::pool as session_pool;
use crate::chat::types::*;

/// 本源模式 (`POLARIS_RAW_MODE=1`) —— 「跟在终端裸调 `claude` 一模一样」。
///
/// 缘由: 常规路径每轮要先拼 19 段 system 注入(KB-First 铁律 / 回答风格 / 输出约定 /
/// 脚本公约 / 检索公约 / 长任务铁律 / KB 召回 / 产物地图 / 历史块 …)再问用户的问题, 还叠一层
/// `--allowedTools` 白名单 + `--disallowedTools` 黑名单。这些约束单看每条都有实证来由,
/// 叠在一起却会把模型压成「事事有回应、事事无着落」—— 尤其非 Claude 系的弱模型会严格照字面
/// 执行到荒谬(见 prompt.rs `kb_first_directive` 的 A/B 实证)。本模式给出一个干净逃生舱:
///
/// - **零注入**: 丢弃全部指令段与召回块, 只把用户原话经 stdin 交给 CLI(连 `## 用户问题`
///   这个包装标题都不加);
/// - **不限工具**: 不传 `--allowedTools` / `--disallowedTools`, 工具面由 CLI 自己按
///   `--permission-mode` 与用户 `~/.claude` 配置决定;
/// - **不额外放行目录**: 不传 `--add-dir`, 只信任 cwd 子树 —— 与裸 CLI 行为一致。
///
/// 剩下的 argv 与 MicaBase(youxi)那套裸调完全对齐:
/// `--print --output-format stream-json --verbose --include-partial-messages`。
///
/// **代价(有意为之)**: 知识库不再自动召回、成品不再自动落产物目录(侧边栏预览也就看不到)、
/// 脚本/长任务/下载那几条实证公约一并失效。要这些能力就别开本模式。
///
/// **为什么用进程级 env 而不是对话内开关**: 常驻会话池按 [`session_pool::Fingerprint`]
/// 判是否复用进程, 而工具白名单不在指纹字段里。做成可随时翻转的 UI 开关, 会让翻转后
/// 旧进程(带着老 argv)被继续复用 → 行为对不上。env 在进程存活期恒定, 天然没有这个坑;
/// 要接 UI 开关得给 Fingerprint 加一个字段(那在 session_pool.rs, 不在本文件)。
/// 改这个变量后**重启应用**生效。
pub(crate) fn raw_mode() -> bool {
    std::env::var("POLARIS_RAW_MODE")
        .map(|v| {
            let t = v.trim();
            !t.is_empty() && t != "0"
        })
        .unwrap_or(false)
}

/// 默认预授权的联网工具 (逗号分隔, 传给 `--allowedTools`)。
/// 把内置 WebSearch / WebFetch 设为「联网搜索默认打开」: 任何权限模式都不再拦截,
/// 深度搜索 / 联网搜索因此能真正联网检索, 而不是退回内置知识。
const DEFAULT_WEB_TOOLS: &str = "WebSearch,WebFetch";

/// 非「拒绝授权」档位下额外放行的本地工具。
/// 缘由: headless (`--print`, stdin=null) 模式下没有人能逐个点「同意」, `acceptEdits`
/// 只自动批准文件编辑而 **不含执行**, 于是 claude 能写出 `create_pptx.py` 却跑不了
/// `python create_pptx.py` → .pptx / .xlsx / 图表这类「要执行脚本才能产出」的成品全部卡死
/// (实测 permission_denials 五连拒, 工具名是 Windows 的 `PowerShell`)。
/// 这里显式放行本地读写 + 执行 (Windows shell 工具叫 `PowerShell`, 跨平台再带上 `Bash`),
/// 让成品能真正落地。危险兜底仍由「拒绝授权(plan, 只读)」档位提供。
/// NotebookEdit 也入列: 工作模式(纯 Claude Code)本就放开全套工具, headless 下不在
/// allowedTools 里等于被拒 —— 快速模式仍经 disallowedTools 砍掉(其优先级更高)。
const LOCAL_WORK_TOOLS: &str = "Read,Write,Edit,Glob,Grep,Bash,PowerShell,NotebookEdit";

/// 按权限档位 (cli_value: default | acceptEdits | bypassPermissions | plan) 组装 `--allowedTools`。
/// - plan (拒绝授权 / 只读): 仅联网工具, 不放行任何本地执行;
/// - default / acceptEdits (手动 / 自动): 联网 + 本地读写执行, 成品能真正产出;
/// - bypassPermissions (完全放行): 模式本身已免确认, allowedTools 仅是冗余声明。
/// - with_task=true (动态编排): 额外放行 `Task` —— 否则 headless(stdin=null)下编排器
///   想扇出子代理会卡在权限确认上, 多智能体并行就跑不起来。
fn allowed_tools_for(perm: &str, with_task: bool) -> String {
    let mut tools = if perm == "plan" {
        DEFAULT_WEB_TOOLS.to_string()
    } else {
        format!("{},{}", DEFAULT_WEB_TOOLS, LOCAL_WORK_TOOLS)
    };
    if with_task {
        tools.push_str(",Task");
    }
    tools
}

/// 快速模式弃用的冗余工具(传给 `--disallowedTools`)。
/// 快速模式 = 快速调用知识库 + 快速回答, 工具面要小: 砍掉多智能体扇出(Task, 启动慢且贵)、
/// Jupyter(NotebookEdit, 纯数据科学)。Read/Write/Edit/Glob/Grep/Bash/Web 都保留 —— 仍能查库、
/// 看文件、做轻量产出; 检索提速靠 `search_convention` + KB 根 `.ignore`(ripgrep 缩范围), 不靠砍
/// Glob/Grep。disallowedTools 优先级高于 allowedTools, 即便 acceptEdits 也拦得住。
/// 返回 None = 工作模式(纯 Claude Code, 放开全套工具)。with_task(动态编排)为真时不禁 Task。
fn disallowed_tools_for(work_full: bool, with_task: bool) -> Option<String> {
    if work_full {
        return None;
    }
    let mut tools = vec!["NotebookEdit"];
    if !with_task {
        tools.push("Task");
    }
    Some(tools.join(","))
}

/// 创作型 skill: 任务 = 做成品(PPT/网页/视频/图),不是知识问答。命中任一(显式点选或
/// 意图自动激活)即进入「创作模式」: 豁免 KB-First 强制取证 + Codex 扁平回复风格
/// (两者的「先查库再作答」「压缩字数」倾向会稀释创作注意力、把成品文案写干瘪 ——
/// 实测「软件内做 PPT 不如终端裸跑 claude」的主因), 只保留数据/指令隔离安全条款。
const CREATIVE_SKILL_IDS: &[&str] = &[
    "polaris-deck-studio",
    "polaris-web-studio",
    "polaris-video-studio",
    "web-video-presentation",
    "pptx",
    "image-gen",
];

// ───────────────────────── State ─────────────────────────

// 子进程池 + 「取消挂起」标记已收口到 runtime::procs::CHILDREN(与 doctor 共池,
// req_id 前缀不同不冲突);「spawn 完成前点停止」的窄窗口语义见 ChildRegistry 文档。
use crate::runtime::procs::{kill_tree, CHILDREN};

/// 单轮 assistant 文本落库缓冲上限 (字节): 防 claude 异常死循环狂打输出把内存撑爆。
/// 超限后实时 delta 仍照常 emit 给前端, 只是不再增长落库缓冲, 末尾加一次截断标记。
const MAX_ASSISTANT_BYTES: usize = 8 * 1024 * 1024;
/// 单轮 stderr 累积上限 (字节)。
const MAX_STDERR_BYTES: usize = 1024 * 1024;

// ───────────────────────── Commands ──────────────────────


mod prepare;
mod process;
mod spawn;
mod sampling;
mod stream;
mod parser;
mod finish;
#[cfg(test)]
mod tests;
pub(crate) use prepare::*;
use process::*;
pub(crate) use spawn::*;
pub(crate) use sampling::*;
pub(crate) use stream::*;
use parser::*;

pub(crate) fn emit_event(app: &AppHandle, ev: ChatStreamEvent) {
    if !crate::chat::backend::claude_cli::capture(&ev) {
        let _ = app.emit("chat:stream", ev);
    }
}

pub(crate) fn send(app: &AppHandle, req_id: &str, args: ChatSendArgs) -> Result<(), String> {
    if CHILDREN.take_cancel(req_id) {
        emit_event(app, crate::chat::backend::event(req_id, args.conversation_id.as_deref(), "done", None));
        return Ok(());
    }
    let Prepared { args, art_dir, art_before, project_work_dir, plan, image_notice, work_full, creative } = prepare(args);
    let conv_id_opt = args.conversation_id.clone();

    // 上下文预算自检: 估算本轮注入的总 token 并 emit 给前端(kind=meta) —— 分批编排据此
    // 自适应批量大小(input 越大则每批越小), 也让「自动检测上下文优化」有据可依。
    // 常驻续轮实际只写增量, 但 claude 的**有效上下文**仍 ≈ 全量(进程内自持), 报全量口径
    // 对「上下文预算」语义才是对的。
    let full_prompt = plan.full_text();
    let est_tokens = estimate_tokens(&full_prompt);
    emit_event(
        app,
        ChatStreamEvent {
            call_id: None, parent_call_id: None,
            req_id: req_id.to_string(),
            kind: "meta".into(),
            text: Some(est_tokens.to_string()),
            tool: None,
            conversation_id: conv_id_opt.clone(),
        },
    );

    // spawn 前再查一次「取消挂起」: 上面拼 prompt + KB 召回可能耗时数秒, 期间用户可能
    // 已点停止 —— 有标记就直接放弃, 连子进程都不起。
    if CHILDREN.take_cancel(req_id) {
        emit_event(app, crate::chat::backend::event(req_id, args.conversation_id.as_deref(), "done", None));
        return Ok(());
    }


    run_process(app, req_id, args, art_dir, art_before, project_work_dir, plan, image_notice, work_full, creative, full_prompt)

}

pub(crate) struct Prepared {
    pub args: ChatSendArgs,
    pub art_dir: std::path::PathBuf,
    pub art_before: std::collections::HashMap<std::path::PathBuf, SystemTime>,
    pub project_work_dir: Option<String>,
    pub plan: session_pool::PromptPlan,
    pub image_notice: Option<String>,
    pub work_full: bool,
    pub creative: bool,
}

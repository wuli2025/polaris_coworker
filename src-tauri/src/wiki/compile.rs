//! 构建知识网 kb_compile + headless claude 只读管线 + JSON 提取 —— 自原 kb.rs 纯移动, 逻辑零改动。
//! (Phase 0 已归位 wiki 域; 共享作用域仍取自 kb 门面, 与原 `use super::*` 同一语义。)

use crate::kb::*;

// ───────────────────────── 构建知识网 (摄入即编译 Ingest=Compile) ─────────────────────────
//
// Karpathy LLM-Wiki 的核心是「写的那一半」: 摄入资料时让 LLM 读原文、抽实体/概念、
// 在 wiki/ 写页面、落 [[双链]]、记账 index/log —— 交叉引用「早就写好了」, 知识因此互联成网。
// 旧「构建索引」(kb_scan) 只重扫文件、刷新内存, 不产生任何新知识与新关联。
// kb_compile 就是补上的编译器: 复用 chat.rs 已验证的 headless `claude --print` 管线,
// 给一个带写权限(Read/Write/Edit/Glob/Grep)的 claude 进程当「wiki 维护者」, 让它自己
// Read 原文、Write wiki 页 —— 与现有架构天然契合, 不引入新的 LLM API / 向量依赖。

pub(crate) static KB_COMPILE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// 知识库维护互斥: compile / enrich_links / dedup 三者都 spawn 后台线程改写同一批 wiki
/// 文件(读-改-写)。并发跑会互相覆盖(lost update)甚至 dedup 删文件时 enrich 正在写它。
/// 用一个全局忙标志串行化, RAII guard 在线程结束(Drop)时自动释放。
pub(crate) static KB_TASK_BUSY: AtomicBool = AtomicBool::new(false);

pub(crate) struct KbTaskGuard;
impl Drop for KbTaskGuard {
    fn drop(&mut self) {
        KB_TASK_BUSY.store(false, Ordering::SeqCst);
    }
}
/// 抢占维护锁; 已有任务在跑则返回 Err(前端可提示稍候)。把返回的 guard `move` 进后台线程,
/// 线程跑完(正常/出错/panic)都会 Drop 释放。
pub(crate) fn acquire_kb_task() -> Result<KbTaskGuard, String> {
    if KB_TASK_BUSY
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("已有知识库维护任务在运行, 请等它结束后再试".into());
    }
    Ok(KbTaskGuard)
}

/// KB 内容原子落盘: 临时文件 + rename(同卷原子)。dedup/enrich 改写 wiki 页时若裸 fs::write
/// 中途崩溃会把页面截成半截, 丢失 AI/用户内容。统一走这里。
pub(crate) fn kb_atomic_write(path: &Path, contents: &str) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".polaris.tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, contents)?;
    fs::rename(&tmp, path)
}

/// 编译进度事件 (前端「构建知识网」进度面板订阅 `kb:compile`)。
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KbCompileEvent {
    pub run_id: String,
    /// phase | tool | page | delta | done | error
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// 仅 done 事件: 编译后重扫得到的文档总数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc_count: Option<usize>,
}

pub(crate) fn emit_compile(app: &AppHandle, run_id: &str, kind: &str, text: Option<String>) {
    let _ = app.emit(
        "kb:compile",
        KbCompileEvent {
            run_id: run_id.into(),
            kind: kind.into(),
            text,
            doc_count: None,
        },
    );
}

/// 「wiki 维护者」system prompt —— Karpathy 式「摄入即编译」。clean-room 自写, 只学方法论。
pub(crate) fn compile_directive(root_disp: &str) -> String {
    format!(
        "# 角色：知识库 wiki 维护者 (Karpathy 式 LLM-Wiki)\n\n\
你是这个知识库的**维护者**。知识库根目录就在你的工作目录: `{root}`。\n\
它分三层:\n\
- `raw/` — 原始资料, **只读, 严禁写入或修改**。\n\
- `wiki/` — **由你全权拥有的知识层**: 摘要页 / 实体页 / 概念页 / 综合页。你在这里写。\n\
- `output/` — 生成的报告类成品。\n\n\
## 你这一轮的任务：摄入即编译 (Ingest = Compile)\n\n\
把 `raw/` 里的原始资料**编译**成一张互联的知识网, 而不是简单罗列。具体:\n\n\
1. **先读规则与现状**: 读 `CLAUDE.md`(若有) 了解约定; 读 `wiki/index.md` 和 `wiki/` 下已有页面, 知道已经有什么。\n\
2. **盘点资料**: 用 Glob/Grep 扫 `raw/`, 了解有哪些资料、主题是什么。**不要逐篇全文读**, 靠文件名和 Grep 抽样了解即可, 控制成本。\n\
3. **抽取并撰写知识 (核心)**: 识别贯穿资料的**实体**(人/地/组织/事件)与**概念/思想脉络**(反复出现的主题、论点)。\
概念页放 `wiki/概念/`、实体页放 `wiki/实体/`(没有就新建子目录); 在页面里**用 `[[页面标题]]` 双链**指向相关的其它 wiki 页, 并用 Grep 找出哪些 raw 篇目讲了它、列进 frontmatter 的 `sources` 并在正文引用。\
这一步的目的是**建立关联**: 原本互不相连的资料, 经由共同的概念页/实体页被串成网。\n\
4. **记账**: 更新 `wiki/index.md` (每个 wiki 页一行: `- [[标题]] — 一句话摘要`, 按类型分组);\
追加 `wiki/log.md` (一行: `## [今天日期] compile | 本轮做了什么`, 没有就新建)。\n\n\
## 页面格式 (每个新建/更新的 wiki 页都要带 frontmatter)\n\n\
```\n\
---\n\
title: 页面标题\n\
type: concept        # entity | concept | source | synthesis 之一\n\
sources: [\"raw/某资料.md\"]   # 这页依据的原始资料相对路径, 可多个\n\
---\n\
\n\
正文... 用 [[其它页面]] 互联, 用脚注/引用标注来源, 不要编造 raw/ 里没有的事实。\n\
```\n\n\
## 针对「语料型」知识库 (如大量同质篇目、彼此几乎无双链)\n\n\
不要逐篇浅摘就完事。**优先抽思想脉络的概念页**(例如把反复出现的主题各立一个概念页),\
在概念页里用 `[[…]]` 把相关篇目链接进来 —— 让原本散落的篇目经由概念层互联成脉络。\
这一轮重在**覆盖度与连接**(把散点连成网), 不必把每篇都深挖到底。\n\n\
## 硬约束\n\n\
- **绝不修改或写入 `raw/`**。只读它。\n\
- 不编造资料里没有的内容; 拿不准的事写进 `wiki/` 时标注「待核实」。\n\
- 双链统一用 `[[页面标题]]` 形式 (标题=对应 wiki 文件名去掉 .md)。\n\
- 全程用中文撰写 wiki 页。\n\n\
完成后, 用一两句话总结你**新建/更新了哪些 wiki 页**、建立了哪些关联。现在开始。",
        root = root_disp
    )
}

#[cfg_attr(not(windows), allow(unused_variables))]
pub(crate) fn compile_no_window(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: GUI 进程 spawn 控制台子进程时不弹黑窗
        cmd.creation_flags(0x0800_0000);
    }
}

/// 「构建知识网」: 启动一个有写权限的 headless claude 当 wiki 维护者, 把 raw/ 编译进 wiki/。
/// 立即返回 run_id; 进度通过 `kb:compile` 事件流式推送, 完成时发 `done` (附重扫后的文档数)。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn kb_compile(app: AppHandle) -> Result<String, String> {
    let root = KB_ROOT.read().clone();
    if root.as_os_str().is_empty() || !root.exists() {
        return Err("知识库根目录不存在, 请先在「管理」里设置".into());
    }
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let c = KB_COMPILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let run_id = format!("kbc-{:x}-{:x}", ts, c);

    let claude_bin: std::ffi::OsString = crate::doctor::resolve_claude_exe()
        .map(|p| p.into_os_string())
        .unwrap_or_else(|| "claude".into());
    let root_disp = root.to_string_lossy().replace('\\', "/");
    let prompt = compile_directive(&root_disp);

    let _kb_task = acquire_kb_task()?;
    let run_id_thread = run_id.clone();
    std::thread::spawn(move || {
        let _kb_task = _kb_task; // 持锁直到本线程结束(Drop 释放)
        emit_compile(
            &app,
            &run_id_thread,
            "phase",
            Some("启动 wiki 维护者…".into()),
        );

        // prompt 经 stdin 喂给 claude (而非命令行参数): 大 prompt 不会撞 Windows 命令行
        // 长度上限, 也不会因 prompt 以 `-` 开头被当成 flag —— 实测 argv 路径在某些 shell 下
        // 会触发 claude 的「Input must be provided」直接退 1, stdin 管道稳。
        let mut cmd = Command::new(&claude_bin);
        cmd.args([
            "--print",
            "--output-format",
            "stream-json",
            "--verbose",
            "--permission-mode=bypassPermissions",
            "--allowedTools",
            "Read,Write,Edit,Glob,Grep",
        ])
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
        crate::doctor::harden_child_env(&mut cmd); // loopback NO_PROXY + 清干扰变量
        crate::provider::scope_child_claude(&mut cmd); // 隔离模式第三方 → 私有会话账本
        compile_no_window(&mut cmd);

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                emit_compile(
                    &app,
                    &run_id_thread,
                    "error",
                    Some(format!("调起 claude 失败: {e}")),
                );
                let _ = app.emit(
                    "kb:compile",
                    KbCompileEvent {
                        run_id: run_id_thread.clone(),
                        kind: "done".into(),
                        text: Some("编译未启动".into()),
                        doc_count: None,
                    },
                );
                return;
            }
        };

        // 把 prompt 写进 stdin 并关闭 (drop 即关), claude 读到 EOF 后开始干活
        if let Some(mut si) = child.stdin.take() {
            use std::io::Write as _;
            let _ = si.write_all(prompt.as_bytes());
            // si 在此作用域结束时 drop → stdin 关闭, 触发 claude 开始处理
        }

        // stderr: 累积, 退出非零时给原因 (stream-json 模式下通常为空, 仅崩溃时有内容)
        let stderr_buf = std::sync::Arc::new(parking_lot::Mutex::new(String::new()));
        if let Some(se) = child.stderr.take() {
            let buf = stderr_buf.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(se)
                    .lines()
                    .map_while(std::result::Result::ok)
                {
                    if !line.trim().is_empty() {
                        buf.lock().push_str(&line);
                        buf.lock().push('\n');
                    }
                }
            });
        }

        // stdout: 解析 stream-json, 把工具调用 / 写页面 / 文本翻成进度
        let mut pages: Vec<String> = Vec::new();
        if let Some(so) = child.stdout.take() {
            emit_compile(
                &app,
                &run_id_thread,
                "phase",
                Some("读取资料、抽取实体与概念…".into()),
            );
            for line in BufReader::new(so)
                .lines()
                .map_while(std::result::Result::ok)
            {
                if line.trim().is_empty() {
                    continue;
                }
                let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else {
                    continue;
                };
                if v.get("type").and_then(|x| x.as_str()) != Some("assistant") {
                    // result 事件的错误子类型 → 透传
                    if v.get("type").and_then(|x| x.as_str()) == Some("result") {
                        if let Some(st) = v.get("subtype").and_then(|x| x.as_str()) {
                            if st.starts_with("error") {
                                let msg = v
                                    .get("result")
                                    .and_then(|x| x.as_str())
                                    .unwrap_or("(unknown)")
                                    .to_string();
                                emit_compile(
                                    &app,
                                    &run_id_thread,
                                    "error",
                                    Some(format!("[{st}] {msg}")),
                                );
                            }
                        }
                    }
                    continue;
                }
                let Some(content) = v
                    .get("message")
                    .and_then(|m| m.get("content"))
                    .and_then(|c| c.as_array())
                else {
                    continue;
                };
                for block in content {
                    match block.get("type").and_then(|x| x.as_str()) {
                        Some("tool_use") => {
                            let name = block.get("name").and_then(|x| x.as_str()).unwrap_or("");
                            if matches!(name, "Write" | "Edit" | "MultiEdit") {
                                if let Some(fp) = block
                                    .get("input")
                                    .and_then(|i| i.get("file_path"))
                                    .and_then(|x| x.as_str())
                                {
                                    let norm = fp.replace('\\', "/");
                                    let short =
                                        norm.rsplit('/').next().unwrap_or(&norm).to_string();
                                    if !pages.contains(&norm) {
                                        pages.push(norm);
                                    }
                                    emit_compile(
                                        &app,
                                        &run_id_thread,
                                        "page",
                                        Some(format!("写入 {short}")),
                                    );
                                }
                            } else {
                                emit_compile(&app, &run_id_thread, "tool", Some(name.to_string()));
                            }
                        }
                        Some("text") => {
                            if let Some(t) = block.get("text").and_then(|x| x.as_str()) {
                                let t = t.trim();
                                if !t.is_empty() {
                                    emit_compile(
                                        &app,
                                        &run_id_thread,
                                        "delta",
                                        Some(t.to_string()),
                                    );
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        let status = child.wait();
        // 编译完成 → 重扫刷新内存索引 + 图谱
        let root_now = KB_ROOT.read().clone();
        let docs = scan_all(&root_now);
        let n = docs.len();
        *INDEX.write() = docs;

        let ok = matches!(&status, Ok(s) if s.success());
        if !ok {
            let code = status.as_ref().ok().and_then(|s| s.code());
            let se = stderr_buf.lock().clone();
            emit_compile(
                &app,
                &run_id_thread,
                "error",
                Some(format!(
                    "claude 退出码 {code:?}{}",
                    if se.is_empty() {
                        String::new()
                    } else {
                        format!(" — {se}")
                    }
                )),
            );
        }
        let msg = if ok {
            format!(
                "编译完成: 新建/更新 {} 个页面, 知识库共 {} 篇",
                pages.len(),
                n
            )
        } else {
            "编译中断 (见上方原因), 已刷新索引".into()
        };
        let _ = app.emit(
            "kb:compile",
            KbCompileEvent {
                run_id: run_id_thread.clone(),
                kind: "done".into(),
                text: Some(msg),
                doc_count: Some(n),
            },
        );
    });

    Ok(run_id)
}

// ───────────────────────── 共享: 只读 claude → 收集 JSON (Wave B 基础设施) ─────────────────────────
//
// enrich/dedup 共用的核心模式 (借鉴 llm_wiki「让 AI 只出决策数据, 代码执行改动」):
// 起一个**只读** (allowedTools 仅 Read/Glob/Grep, 物理上无法写文件) 的 headless claude,
// 让它读 wiki、输出一段 JSON 决策, 把全部 assistant 文本收集起来返回。改文件由 Rust 做。

/// 起一个只读 headless claude, 把 prompt 经 stdin 喂进去, 收集其全部 assistant 文本块返回。
/// `on_event(kind, text)`: kind ∈ {tool, delta} 用于向前端透传进度。阻塞直到进程退出。
pub(crate) fn run_claude_readonly<F: FnMut(&str, &str)>(
    root: &Path,
    prompt: &str,
    on_event: F,
) -> Result<String, String> {
    run_claude_readonly_inner(root, prompt, on_event, None)
}

/// 同 `run_claude_readonly`,但带墙钟超时:到点 kill 子进程并整树回收,返回 Err。
/// 用于检索 AI 扩写等「卡住必须能放手」的路径(阻塞线程池有限,不能被永久钉死)。
pub(crate) fn run_claude_readonly_timeout<F: FnMut(&str, &str)>(
    root: &Path,
    prompt: &str,
    on_event: F,
    timeout: std::time::Duration,
) -> Result<String, String> {
    run_claude_readonly_inner(root, prompt, on_event, Some(timeout))
}

pub(crate) fn run_claude_readonly_inner<F: FnMut(&str, &str)>(
    root: &Path,
    prompt: &str,
    mut on_event: F,
    timeout: Option<std::time::Duration>,
) -> Result<String, String> {
    let claude_bin: std::ffi::OsString = crate::doctor::resolve_claude_exe()
        .map(|p| p.into_os_string())
        .unwrap_or_else(|| "claude".into());
    let mut cmd = Command::new(&claude_bin);
    cmd.args([
        "--print",
        "--output-format",
        "stream-json",
        "--verbose",
        "--permission-mode=bypassPermissions",
        "--allowedTools",
        "Read,Glob,Grep", // 只读: 物理上不给 Write/Edit, 决策数据落地由 Rust 执行
    ])
    .current_dir(root)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    crate::doctor::harden_child_env(&mut cmd); // loopback NO_PROXY + 清干扰变量
    crate::provider::scope_child_claude(&mut cmd); // 隔离模式第三方 → 私有会话账本
    compile_no_window(&mut cmd);

    let mut child = cmd.spawn().map_err(|e| format!("调起 claude 失败: {e}"))?;
    if let Some(mut si) = child.stdin.take() {
        use std::io::Write as _;
        let _ = si.write_all(prompt.as_bytes());
    }
    let stderr_buf = std::sync::Arc::new(parking_lot::Mutex::new(String::new()));
    if let Some(se) = child.stderr.take() {
        let buf = stderr_buf.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(se)
                .lines()
                .map_while(std::result::Result::ok)
            {
                if !line.trim().is_empty() {
                    buf.lock().push_str(&line);
                    buf.lock().push('\n');
                }
            }
        });
    }

    let stdout = child.stdout.take();

    // 墙钟看门狗:设了 timeout 时,到点 kill 子进程 —— stdout 随之关闭,下面的读循环自然结束。
    // 命令正常读完会 drop done_tx 让看门狗提前醒来(不空等满 timeout);None=不设超时(旧行为)。
    let child = std::sync::Arc::new(parking_lot::Mutex::new(child));
    let timed_out = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
    let watchdog = timeout.map(|dur| {
        let child_w = std::sync::Arc::clone(&child);
        let flag = std::sync::Arc::clone(&timed_out);
        std::thread::spawn(move || {
            if matches!(
                done_rx.recv_timeout(dur),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout)
            ) {
                flag.store(true, std::sync::atomic::Ordering::SeqCst);
                let _ = child_w.lock().kill();
            }
        })
    });

    let mut collected = String::new();
    let mut result_err: Option<String> = None;
    if let Some(so) = stdout {
        for line in BufReader::new(so)
            .lines()
            .map_while(std::result::Result::ok)
        {
            if line.trim().is_empty() {
                continue;
            }
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            let ty = v.get("type").and_then(|x| x.as_str()).unwrap_or("");
            if ty == "result" {
                if let Some(st) = v.get("subtype").and_then(|x| x.as_str()) {
                    if st.starts_with("error") {
                        result_err = Some(format!("claude 返回错误: {st}"));
                        break;
                    }
                }
                continue;
            }
            if ty != "assistant" {
                continue;
            }
            let Some(content) = v
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_array())
            else {
                continue;
            };
            for block in content {
                match block.get("type").and_then(|x| x.as_str()) {
                    Some("tool_use") => {
                        let name = block.get("name").and_then(|x| x.as_str()).unwrap_or("");
                        on_event("tool", name);
                    }
                    Some("text") => {
                        if let Some(t) = block.get("text").and_then(|x| x.as_str()) {
                            collected.push_str(t);
                            on_event("delta", t.trim());
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // 读到 EOF(或 error 提前 break):通知看门狗退出,再回收子进程(避免僵尸 + 线程泄漏)。
    drop(done_tx);
    if let Some(h) = watchdog {
        let _ = h.join();
    }
    let status = child.lock().wait();

    if timed_out.load(std::sync::atomic::Ordering::SeqCst) {
        let secs = timeout.map(|d| d.as_secs()).unwrap_or_default();
        return Err(format!("claude 超时({secs}s)已终止"));
    }
    if let Some(e) = result_err {
        return Err(e);
    }
    if !matches!(&status, Ok(s) if s.success()) {
        let se = stderr_buf.lock().clone();
        return Err(format!(
            "claude 异常退出{}",
            if se.is_empty() {
                String::new()
            } else {
                format!(": {se}")
            }
        ));
    }
    Ok(collected)
}

/// 从一段文本里抽出第一个**平衡**的 JSON (对象 `{...}` 或数组 `[...]`), 容忍前后包裹的
/// markdown 代码围栏与说明文字 (借鉴 llm_wiki 对 LLM 输出格式宽松解析)。
pub(crate) fn extract_balanced_json(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let start = s.find(['{', '['])?;
    let open = bytes[start];
    let close = if open == b'{' { b'}' } else { b']' };
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if in_str {
            if esc {
                esc = false;
            } else if b == b'\\' {
                esc = true;
            } else if b == b'"' {
                in_str = false;
            }
            continue;
        }
        match b {
            b'"' => in_str = true,
            x if x == open => depth += 1,
            x if x == close => {
                depth -= 1;
                if depth == 0 {
                    return Some(s[start..=i].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_json_tolerates_fences_and_prose() {
        let s = "好的, 结果如下:\n```json\n[{\"a\":1},{\"b\":\"]x\"}]\n```\n完毕";
        let j = extract_balanced_json(s).unwrap();
        assert_eq!(j, "[{\"a\":1},{\"b\":\"]x\"}]");
        let obj = extract_balanced_json("noise {\"k\": \"v}v\"} tail").unwrap();
        assert_eq!(obj, "{\"k\": \"v}v\"}");
    }
}
